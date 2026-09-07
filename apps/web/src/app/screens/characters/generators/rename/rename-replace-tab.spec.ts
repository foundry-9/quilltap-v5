import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { ToastService } from '../../../../ui/toast.service';
import fixtureResponse from './fixtures/rename-preview-response.json';
import { CharacterRenameReplaceTab } from './rename-replace-tab';

type Req = { type: string; [k: string]: unknown };

function toastMessages(): string[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => t.message);
}

function stubClient(seen: Req[], response: unknown = fixtureResponse): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      seen.push(req);
      if (req.type === 'characterRename') {
        return req['dryRun'] === false ? { ...(response as object), dryRun: false } : response;
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<CharacterRenameReplaceTab>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CharacterRenameReplaceTab],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(CharacterRenameReplaceTab);
  fixture.componentRef.setInput('characterId', 'char-1');
  fixture.componentRef.setInput('characterName', 'Snips');
  fixture.detectChanges();
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function clickButtonWithText(fixture: ComponentFixture<unknown>, text: string): void {
  const button = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === text,
  ) as HTMLButtonElement | undefined;
  if (!button) throw new Error(`no button with text "${text}"`);
  button.click();
}

describe('CharacterRenameReplaceTab — preview grouping/counts (v4 `RenameReplaceTab.tsx`)', () => {
  it('a Preview renders the summary counts table in v4 column order and grouped replacements', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen));

    // Enter a new name so Preview is enabled (v4 `hasValidInput`).
    const newNameInput = (fixture.nativeElement as HTMLElement).querySelector(
      'input[placeholder="Enter new name"]',
    ) as HTMLInputElement;
    newNameInput.value = 'Ace';
    newNameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    clickButtonWithText(fixture, 'Preview Changes');
    await settle(fixture);

    const req = seen.find((r) => r.type === 'characterRename');
    expect(req).toMatchObject({
      characterId: 'char-1',
      dryRun: true,
      primaryRename: { oldValue: 'Snips', newValue: 'Ace', caseSensitive: false },
    });

    const textNow = (fixture.nativeElement as HTMLElement).textContent ?? '';
    // v4's summary column order (Character Fields, Descriptions, Memories,
    // Chat Titles, Messages, Total) — asserted by finding each count next to
    // its label in DOM ORDER, not just presence.
    const order = ['Character Fields', 'Descriptions', 'Memories', 'Chat Titles', 'Messages', 'Total'];
    let lastIndex = -1;
    for (const label of order) {
      const idx = textNow.indexOf(label);
      expect(idx, `${label} present`).toBeGreaterThan(-1);
      expect(idx, `${label} in v4 column order`).toBeGreaterThan(lastIndex);
      lastIndex = idx;
    }
    expect(textNow).toContain('Replacements (6)');
    expect(textNow).toContain(fixtureResponse.summary.total.toString());
  });

  it('Execute confirms, dispatches dryRun:false, and toasts the v4 sentence', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen));
    window.confirm = (() => true) as typeof window.confirm;

    const newNameInput = (fixture.nativeElement as HTMLElement).querySelector(
      'input[placeholder="Enter new name"]',
    ) as HTMLInputElement;
    newNameInput.value = 'Ace';
    newNameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    clickButtonWithText(fixture, 'Preview Changes');
    await settle(fixture);

    clickButtonWithText(fixture, `Execute ${fixtureResponse.summary.total} Replacements`);
    await settle(fixture);

    const executeReq = seen.filter((r) => r.type === 'characterRename').at(-1);
    expect(executeReq?.['dryRun']).toBe(false);
    expect(toastMessages()).toContain(`Successfully updated ${fixtureResponse.summary.total} occurrences!`);
  });

  it('Execute does nothing when the confirm is declined', async () => {
    const seen: Req[] = [];
    const fixture = await render(stubClient(seen));
    window.confirm = (() => false) as typeof window.confirm;

    const newNameInput = (fixture.nativeElement as HTMLElement).querySelector(
      'input[placeholder="Enter new name"]',
    ) as HTMLInputElement;
    newNameInput.value = 'Ace';
    newNameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    clickButtonWithText(fixture, 'Preview Changes');
    await settle(fixture);
    const beforeExecuteCount = seen.filter((r) => r.type === 'characterRename').length;

    clickButtonWithText(fixture, `Execute ${fixtureResponse.summary.total} Replacements`);
    await settle(fixture);

    expect(seen.filter((r) => r.type === 'characterRename').length).toBe(beforeExecuteCount);
  });

  it('shows the empty-results copy when a preview finds nothing (v4 `:420-423`)', async () => {
    const seen: Req[] = [];
    const empty = { ...fixtureResponse, replacements: [], summary: { ...fixtureResponse.summary, total: 0 } };
    const fixture = await render(stubClient(seen, empty));

    const newNameInput = (fixture.nativeElement as HTMLElement).querySelector(
      'input[placeholder="Enter new name"]',
    ) as HTMLInputElement;
    newNameInput.value = 'Ace';
    newNameInput.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    clickButtonWithText(fixture, 'Preview Changes');
    await settle(fixture);

    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('No occurrences found for the specified replacements.');
    // No Execute button when total is 0 (v4 `:427`).
    const executeButton = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
    ).find((b) => b.textContent?.includes('Execute'));
    expect(executeButton).toBeUndefined();
  });
});
