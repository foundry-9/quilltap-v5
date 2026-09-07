/**
 * NewChatPage hosting (P4.d16 tier 2) — the v5-only `salon-new` tab.
 *
 * v4 opens New Chat as a modal seeded with `{ projectId?, characterId?,
 * autonomous }`; v5 hosts this screen as a tab and hands it the same three seeds
 * as inputs. This covers the translation: the seeds win over (absent) query
 * params, and Back / Cancel close the tab the way v4's modal dismisses.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { WORKSPACE_HANDLE, WORKSPACE_TAB_ID } from '../../workspace/workspace-contract';
import { NewChatPage } from './new-chat-page';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function stubClient(): Partial<CoreClient> {
  const handle = (req: DispatchReq): unknown => {
    switch (req.type) {
      case 'characterList':
        return { characters: [] };
      case 'connectionProfileList':
        return { connectionProfiles: [] };
      case 'scenarioList':
        return { scenarios: [] };
      case 'projectList':
        return { projects: [] };
      default:
        return {};
    }
  };
  return {
    dispatchData: (async (req: DispatchReq) => handle(req) as Record<string, unknown>) as
      CoreClient['dispatchData'],
    dispatchExpect: (async (req: DispatchReq) => handle(req)) as CoreClient['dispatchExpect'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

describe('NewChatPage (hosted as the salon-new tab)', () => {
  function render(inputs: Record<string, unknown>, closeTab = vi.fn()) {
    TestBed.configureTestingModule({
      imports: [NewChatPage],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: stubClient() },
        { provide: WORKSPACE_TAB_ID, useValue: 'tab-new' },
        {
          provide: WORKSPACE_HANDLE,
          useValue: { openTab: vi.fn(() => 't'), closeTab, refreshTab: vi.fn() },
        },
      ],
    });
    const fixture = TestBed.createComponent(NewChatPage);
    for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
    fixture.detectChanges();
    return { fixture, closeTab };
  }

  it('takes the autonomous seed from the input (v4 modal prop), not a query param', async () => {
    const { fixture } = render({ autonomous: true });
    await settle(fixture);
    expect(fixture.nativeElement.textContent).toContain('New Autonomous Room');
  });

  it('is a plain New Chat with no seeds', async () => {
    const { fixture } = render({});
    await settle(fixture);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('New Chat');
    expect(text).not.toContain('New Autonomous Room');
  });

  it('Back and Cancel close the tab instead of navigating (v4 modal dismissal)', async () => {
    const { fixture, closeTab } = render({});
    await settle(fixture);
    // No back/cancel anchors survive in tab mode.
    expect(fixture.nativeElement.querySelector('a[href="/salon"]')).toBeNull();
    const buttons = [...fixture.nativeElement.querySelectorAll('button')] as HTMLButtonElement[];
    const back = buttons.find((b) => b.textContent?.includes('Back to'));
    const cancel = buttons.find((b) => b.textContent?.trim() === 'Cancel');
    expect(back).toBeTruthy();
    expect(cancel).toBeTruthy();
    back!.click();
    cancel!.click();
    expect(closeTab).toHaveBeenCalledWith('tab-new');
    expect(closeTab).toHaveBeenCalledTimes(2);
  });
});
