/**
 * BrahmaConsoleDialog — the always-on floating console (v4
 * `BrahmaConsoleDialog.tsx` at `236c04e35`, `99463fc56`): the panel inside a
 * topmost floating dialog, the panel's state surviving the dialog being
 * hidden, and the floating surface on its own conversation apart from the
 * workspace tab's. The panel's own behaviour is `brahma-console-panel.spec.ts`.
 */
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ScopedEvent } from '../core/core-contract';
import { BrahmaConsoleDialog } from './brahma-console-dialog';
import {
  STORAGE_KEY_FLOATING_LAST_CHAT,
  STORAGE_KEY_TAB_LAST_CHAT,
} from './brahma-console-session';
import { BrahmaConsoleView } from './brahma-console-view';
import { BrahmaConsoleService } from './brahma-console.service';

beforeAll(() => {
  const proto = globalThis.HTMLElement?.prototype as unknown as { scrollIntoView?: () => void };
  if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined;
  if (!('ResizeObserver' in globalThis)) {
    (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  }
});

const PROFILES = [
  { id: 'p1', name: 'Everyday', provider: 'OPENAI', modelName: 'gpt-4o', isDefault: true },
];

function configure(seen: { type: string; chatId?: string }[]): void {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [BrahmaConsoleDialog, BrahmaConsoleView],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      {
        provide: CoreClient,
        useValue: {
          events$: new Subject<ScopedEvent>().asObservable(),
          dispatchExpect: async () => ({
            type: 'connectionProfiles',
            data: { profiles: PROFILES, count: 1 },
          }),
          dispatchData: async (req: { type: string; chatId?: string }) => {
            seen.push(req);
            switch (req.type) {
              case 'brahmaConsoleList':
                return { chats: [] };
              case 'brahmaConsoleMessages':
                return {
                  messages: [
                    {
                      id: `m-${req.chatId}`,
                      role: 'USER',
                      content: `in ${req.chatId}`,
                      createdAt: 'x',
                    },
                  ],
                };
              case 'brahmaConsoleGet':
                return { chat: { id: req.chatId, consoleConnectionProfileId: null } };
              default:
                return {};
            }
          },
        },
      },
    ],
  });
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('BrahmaConsoleDialog (v4 BrahmaConsoleDialog.tsx at 236c04e35)', () => {
  afterEach(() => {
    localStorage.clear();
    TestBed.resetTestingModule();
  });

  it('is a topmost, non-modal floating dialog in the floating layer — only while open', async () => {
    configure([]);
    const fixture = TestBed.createComponent(BrahmaConsoleDialog);
    fixture.detectChanges();
    await settle(fixture);
    const el = document;
    expect(el.querySelector('.qt-floating-dialog')).toBeNull();
    TestBed.inject(BrahmaConsoleService).openConsole();
    await settle(fixture);
    const dialog = el.querySelector('.qt-floating-dialog')!;
    expect(dialog.classList.contains('qt-floating-dialog-topmost')).toBe(true);
    expect(dialog.hasAttribute('data-qt-floating-layer')).toBe(true);
    expect(dialog.getAttribute('aria-modal')).toBe('false');
    expect(
      document.getElementById(dialog.getAttribute('aria-labelledby')!)?.textContent?.trim(),
    ).toBe('Brahma Console');
    // The chrome sits in the title bar.
    expect(
      dialog.querySelector('.qt-floating-dialog-actions qt-brahma-model-picker'),
    ).not.toBeNull();
  });

  it('hidden, the panel’s state SURVIVES: reopen → the same chat, no transcript refetch', async () => {
    localStorage.setItem(STORAGE_KEY_FLOATING_LAST_CHAT, 'float-1');
    const seen: { type: string; chatId?: string }[] = [];
    configure(seen);
    const fixture = TestBed.createComponent(BrahmaConsoleDialog);
    const service = TestBed.inject(BrahmaConsoleService);
    service.openConsole();
    fixture.detectChanges();
    await settle(fixture);
    expect(document.querySelector('qt-floating-dialog')?.textContent).toContain('in float-1');
    const reads = () => seen.filter((r) => r.type === 'brahmaConsoleMessages').length;
    const before = reads();
    service.closeConsole();
    await settle(fixture);
    expect(document.querySelector('.qt-floating-dialog')).toBeNull();
    service.openConsole();
    await settle(fixture);
    expect(document.querySelector('qt-floating-dialog')?.textContent).toContain('in float-1');
    expect(reads()).toBe(before);
  });

  it('the tab and the floating dialog sit on two different conversations at once', async () => {
    localStorage.setItem(STORAGE_KEY_FLOATING_LAST_CHAT, 'float-1');
    localStorage.setItem(STORAGE_KEY_TAB_LAST_CHAT, 'tab-1');
    configure([]);
    TestBed.inject(BrahmaConsoleService).openConsole();
    const dialog = TestBed.createComponent(BrahmaConsoleDialog);
    const view = TestBed.createComponent(BrahmaConsoleView);
    dialog.detectChanges();
    view.detectChanges();
    await settle(dialog);
    await settle(view);
    expect(document.querySelector('qt-floating-dialog')?.textContent).toContain('in float-1');
    expect(view.nativeElement.textContent).toContain('in tab-1');
    // The tab renders bare with its inline header row.
    expect((view.nativeElement as HTMLElement).querySelector('.qt-floating-dialog')).toBeNull();
    expect(
      (view.nativeElement as HTMLElement).querySelector(
        '.justify-end qt-brahma-console-header-actions',
      ),
    ).not.toBeNull();
  });
});
