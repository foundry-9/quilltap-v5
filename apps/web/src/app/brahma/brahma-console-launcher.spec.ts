/** v4 `components/brahma-console/BrahmaConsoleLauncher.tsx:21-46` at `236c04e35` (`99463fc56`). */
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { BrahmaConsoleLauncher } from './brahma-console-launcher';
import { BrahmaConsoleService } from './brahma-console.service';

function render() {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [BrahmaConsoleLauncher],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      {
        provide: CoreClient,
        useValue: {
          dispatchExpect: async () => ({
            type: 'connectionProfiles',
            data: { profiles: [], count: 0 },
          }),
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(BrahmaConsoleLauncher);
  fixture.detectChanges();
  const button = document.querySelector('button.qt-brahma-launcher') as HTMLButtonElement;
  return { fixture, button, service: TestBed.inject(BrahmaConsoleService) };
}

describe('BrahmaConsoleLauncher', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('is the floating-layer launcher, never disabled — even with zero profiles', () => {
    const { button } = render();
    expect(button.className).toBe('qt-brahma-launcher');
    expect(button.hasAttribute('data-qt-floating-layer')).toBe(true);
    expect(button.disabled).toBe(false);
    expect(button.querySelector('qt-icon')!.getAttribute('class')).toContain(
      'qt-brahma-launcher-icon',
    );
  });

  it('toggles the console; label, title and aria-pressed track it', () => {
    const { fixture, button, service } = render();
    expect(button.getAttribute('aria-pressed')).toBe('false');
    expect(button.getAttribute('aria-label')).toBe('Open the Brahma Console');
    expect(button.getAttribute('title')).toBe('Open the Brahma Console');
    button.click();
    fixture.detectChanges();
    expect(service.isOpen()).toBe(true);
    expect(button.getAttribute('aria-pressed')).toBe('true');
    expect(button.getAttribute('aria-label')).toBe('Hide the Brahma Console');
    expect(button.getAttribute('title')).toBe('Hide the Brahma Console');
    expect(button.classList.contains('qt-brahma-launcher-active')).toBe(true);
    button.click();
    fixture.detectChanges();
    expect(service.isOpen()).toBe(false);
    expect(button.classList.contains('qt-brahma-launcher-active')).toBe(false);
  });
});
