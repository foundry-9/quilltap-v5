import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { CoreRequest, CoreResponse } from '../../../core/core-contract';
import { AppearanceTab } from './appearance-tab';

function reset() {
  localStorage.clear();
  const root = document.documentElement;
  delete root.dataset['theme'];
  root.classList.remove('dark', 'light');
  document.getElementById('qt-theme-styles')?.remove();
}

function stubClient(onUpdate?: (req: CoreRequest) => void): Partial<CoreClient> {
  const dispatchExpect = (async (req: CoreRequest) => {
    if (req.type === 'chatSettings') {
      return {
        type: 'chatSettings',
        data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' },
      } as CoreResponse;
    }
    if (req.type === 'chatSettingsUpdate') {
      onUpdate?.(req);
      return {
        type: 'chatSettings',
        data: { avatarDisplayMode: 'ALWAYS', avatarDisplayStyle: 'CIRCULAR' },
      } as CoreResponse;
    }
    return { type: 'ack', data: {} } as CoreResponse;
  }) as CoreClient['dispatchExpect'];
  // ThemeService (root) persists best-effort through dispatch().
  const dispatch = vi.fn(async (): Promise<CoreResponse> => ({ type: 'ack', data: {} }));
  return { dispatchExpect, dispatch };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<AppearanceTab>> {
  TestBed.configureTestingModule({
    imports: [AppearanceTab],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(AppearanceTab);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('AppearanceTab', () => {
  afterEach(reset);

  it('renders the theme grid and applies a pack on click (DOM data-theme updates)', async () => {
    const fixture = await render(stubClient());
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Theme');
    expect(text).toContain('Default');

    const artDeco = Array.from(fixture.nativeElement.querySelectorAll('button')).find((b) =>
      (b as HTMLButtonElement).textContent?.includes('Art Deco'),
    ) as HTMLButtonElement | undefined;
    expect(artDeco).toBeTruthy();
    artDeco!.click();
    fixture.detectChanges();
    expect(document.documentElement.dataset['theme']).toBe('art-deco');
  });

  it('persists an avatar mode change via chatSettingsUpdate', async () => {
    const updates: CoreRequest[] = [];
    const fixture = await render(stubClient((req) => updates.push(req)));
    const neverRadio = Array.from(
      fixture.nativeElement.querySelectorAll('input[name="avatarDisplayMode"]'),
    )[2] as HTMLInputElement;
    neverRadio.dispatchEvent(new Event('change'));
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();

    const update = updates.find((r) => r.type === 'chatSettingsUpdate') as {
      settings: Record<string, unknown>;
    };
    expect(update.settings['avatarDisplayMode']).toBe('NEVER');
  });

  it('toggles the nav theme selector preference', async () => {
    const fixture = await render(stubClient());
    const toggle = fixture.nativeElement.querySelector(
      'button[role="switch"]',
    ) as HTMLButtonElement;
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    toggle.click();
    fixture.detectChanges();
    expect(toggle.getAttribute('aria-checked')).toBe('true');
  });
});
