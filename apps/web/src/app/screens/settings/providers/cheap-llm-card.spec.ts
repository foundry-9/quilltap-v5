import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CoreRequest, CoreResponse } from '../../../core/core-contract';
import { CheapLlmCard } from './cheap-llm-card';

function makeDispatchExpect(
  settings: Record<string, unknown>,
  onUpdate?: (req: CoreRequest) => void,
): CoreClient['dispatchExpect'] {
  return (async (req: CoreRequest) => {
    if (req.type === 'chatSettings') {
      return { type: 'chatSettings', data: settings } as CoreResponse;
    }
    if (req.type === 'connectionProfileList') {
      return { type: 'connectionProfiles', data: { profiles: [], count: 0 } } as CoreResponse;
    }
    if (req.type === 'chatSettingsUpdate') {
      onUpdate?.(req);
      return { type: 'chatSettings', data: settings } as CoreResponse;
    }
    return { type: 'ack', data: {} } as CoreResponse;
  }) as CoreClient['dispatchExpect'];
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CheapLlmCard>> {
  TestBed.configureTestingModule({
    imports: [CheapLlmCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CheapLlmCard);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('CheapLlmCard', () => {
  it('PUT-merges a strategy change into the existing cheapLLMSettings', async () => {
    const updates: CoreRequest[] = [];
    const client: Partial<CoreClient> = {
      dispatchExpect: makeDispatchExpect(
        {
          cheapLLMSettings: {
            strategy: 'PROVIDER_CHEAPEST',
            fallbackToLocal: false,
            embeddingProvider: 'OPENAI',
          },
        },
        (req) => updates.push(req),
      ),
    };
    const fixture = await render(client);

    // Pick the "User Defined" strategy radio.
    const radios = Array.from(
      fixture.nativeElement.querySelectorAll('input[name="cheapLLMStrategy"]'),
    ) as HTMLInputElement[];
    radios[0].dispatchEvent(new Event('change'));
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    expect(updates).toHaveLength(1);
    const settings = (
      updates[0] as unknown as { settings: { cheapLLMSettings: Record<string, unknown> } }
    ).settings.cheapLLMSettings;
    // The whole merged object goes out — strategy changed, other keys preserved.
    expect(settings['strategy']).toBe('USER_DEFINED');
    expect(settings['fallbackToLocal']).toBe(false);
    expect(settings['embeddingProvider']).toBe('OPENAI');
  });
});
