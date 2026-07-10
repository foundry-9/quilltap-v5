import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ApiKeyDto } from '../../../core/core-contract';
import { ApiKeysCard } from './api-keys-card';

function key(over: Partial<ApiKeyDto>): ApiKeyDto {
  return {
    id: 'k1',
    provider: 'OPENAI',
    label: 'My Key',
    isActive: true,
    lastUsed: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    keyPreview: 'sk-…AB12',
    ...over,
  };
}

function stubClient(keys: ApiKeyDto[], over: Partial<CoreClient> = {}): Partial<CoreClient> {
  return {
    dispatchExpect: (async () => ({
      type: 'apiKeys',
      data: { apiKeys: keys, count: keys.length },
    })) as CoreClient['dispatchExpect'],
    dispatchData: (async () => ({ valid: true })) as CoreClient['dispatchData'],
    ...over,
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<ApiKeysCard>> {
  TestBed.configureTestingModule({
    imports: [ApiKeysCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ApiKeysCard);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('ApiKeysCard', () => {
  it('renders the masked keyPreview rows (never a plaintext key)', async () => {
    const fixture = await render(
      stubClient([
        key({ id: 'a', label: 'OpenAI Prod', provider: 'OPENAI', keyPreview: 'sk-…9Z9Z' }),
        key({ id: 'b', label: 'Anthropic', provider: 'ANTHROPIC', keyPreview: 'sk-ant-…QQ' }),
      ]),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Your API Keys (2)');
    expect(text).toContain('OpenAI Prod');
    expect(text).toContain('sk-…9Z9Z');
    expect(text).toContain('OPENAI • sk-…9Z9Z');
  });

  it('shows the empty state with v4 copy', async () => {
    const fixture = await render(stubClient([]));
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('No API keys yet');
    expect(text).toContain('Add one to get started.');
  });

  it('surfaces a successful Test result', async () => {
    const dispatchData = vi.fn(async () => ({ valid: true }));
    const fixture = await render(
      stubClient([key({ id: 'a', label: 'K' })], {
        dispatchData: dispatchData as CoreClient['dispatchData'],
      }),
    );
    const testButton = Array.from(fixture.nativeElement.querySelectorAll('button')).find(
      (b) => (b as HTMLButtonElement).textContent?.trim() === 'Test',
    ) as HTMLButtonElement;
    testButton.click();
    for (let i = 0; i < 4; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(dispatchData).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'apiKeyTest', apiKeyId: 'a' }),
    );
    expect(fixture.nativeElement.textContent).toContain('✓ Key is valid');
  });
});
