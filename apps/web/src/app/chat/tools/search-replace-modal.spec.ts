import { Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { SearchReplaceModal } from './search-replace-modal';

/** Search & Replace (v4 `components/tools/search-replace/**`). */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let previewAnswer: Record<string, unknown> | Error = {
  messageMatches: 3,
  memoryMatches: 1,
  affectedChats: 2,
  affectedMemories: 1,
};
let executeAnswer: Record<string, unknown> | Error = {
  messagesUpdated: 3,
  memoriesUpdated: 1,
  chatsAffected: 2,
  errors: [],
};

function stubClient(): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      sent.push(req);
      if (req['type'] === 'characterList') return { characters: [{ id: 'c1', name: 'Lorian' }] };
      if (req['type'] === 'searchReplacePreview') {
        if (previewAnswer instanceof Error) throw previewAnswer;
        return previewAnswer;
      }
      if (executeAnswer instanceof Error) throw executeAnswer;
      return executeAnswer;
    }) as unknown as CoreClient['dispatchData'],
  };
}

@Component({
  imports: [SearchReplaceModal],
  template: `
    <qt-search-replace-modal
      [chatId]="'chat-1'"
      [chatTitle]="'Solo Voyage'"
      (completed)="completions = completions + 1"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  completions = 0;
  closed = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<Host>, turns = 8): Promise<void> {
  for (let i = 0; i < turns; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The debounce is real time, so a preview needs the clock to move. */
async function settlePreview(fixture: ComponentFixture<Host>): Promise<void> {
  await new Promise((r) => setTimeout(r, 350));
  await settle(fixture);
}

function button(fixture: ComponentFixture<Host>, label: string): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.trim() === label,
  )!;
}

async function typeSearch(fixture: ComponentFixture<Host>, text: string): Promise<void> {
  const box = fixture.nativeElement.querySelector('#qt-sr-find') as HTMLInputElement;
  box.value = text;
  box.dispatchEvent(new Event('input'));
  await settlePreview(fixture);
}

describe('SearchReplaceModal', () => {
  beforeEach(() => {
    sent.length = 0;
    previewAnswer = {
      messageMatches: 3,
      memoryMatches: 1,
      affectedChats: 2,
      affectedMemories: 1,
    };
    executeAnswer = { messagesUpdated: 3, memoriesUpdated: 1, chatsAffected: 2, errors: [] };
  });

  it('opens on the search step, the chat scope already chosen', async () => {
    const fixture = await render();
    expect(fixture.nativeElement.querySelector('#qt-sr-find')).toBeTruthy();
    // Back from here reaches the scope step, where the chat is preselected.
    button(fixture, 'Back').click();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Select Scope');
    expect(
      (fixture.nativeElement.querySelector('input[aria-label="Current Chat"]') as HTMLInputElement)
        .checked,
    ).toBe(true);
  });

  it('sends both include flags EXPLICITLY — an omission would mean the opposite', async () => {
    const fixture = await render();
    const memories = fixture.nativeElement.querySelector(
      'input[aria-label="Include memories"]',
    ) as HTMLInputElement;
    memories.checked = false;
    memories.dispatchEvent(new Event('change'));
    await typeSearch(fixture, 'lighthouse');

    const preview = sent.find((r) => r.type === 'searchReplacePreview')!;
    expect(preview).toEqual({
      type: 'searchReplacePreview',
      scope: { type: 'chat', chatId: 'chat-1' },
      searchText: 'lighthouse',
      replaceText: '',
      includeMessages: true,
      includeMemories: false,
    });
  });

  it('will not advance past search until the preview finds something', async () => {
    previewAnswer = { messageMatches: 0, memoryMatches: 0, affectedChats: 0, affectedMemories: 0 };
    const fixture = await render();
    expect(button(fixture, 'Next').disabled).toBe(true);

    await typeSearch(fixture, 'nothing here');
    expect(fixture.nativeElement.textContent).toContain('No matches found');
    expect(button(fixture, 'Next').disabled).toBe(true);

    previewAnswer = { messageMatches: 2, memoryMatches: 0, affectedChats: 1, affectedMemories: 0 };
    await typeSearch(fixture, 'lighthouse');
    expect(button(fixture, 'Next').disabled).toBe(false);
  });

  it('needs the confirmation tick, and drops it when the operator goes back', async () => {
    const fixture = await render();
    await typeSearch(fixture, 'lighthouse');
    button(fixture, 'Next').click();
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('This action cannot be undone');
    expect(button(fixture, 'Replace All').disabled).toBe(true);

    const tick = fixture.nativeElement.querySelector(
      'input[aria-label="I understand"]',
    ) as HTMLInputElement;
    tick.checked = true;
    tick.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(button(fixture, 'Replace All').disabled).toBe(false);

    button(fixture, 'Back').click();
    fixture.detectChanges();
    button(fixture, 'Next').click();
    fixture.detectChanges();
    expect(button(fixture, 'Replace All').disabled).toBe(true);
  });

  it('"Replace All" executes rather than advancing, and reports the counts', async () => {
    const fixture = await render();
    await typeSearch(fixture, 'lighthouse');
    button(fixture, 'Next').click();
    fixture.detectChanges();
    const tick = fixture.nativeElement.querySelector(
      'input[aria-label="I understand"]',
    ) as HTMLInputElement;
    tick.checked = true;
    tick.dispatchEvent(new Event('change'));
    fixture.detectChanges();

    button(fixture, 'Replace All').click();
    await settle(fixture);

    expect(sent.find((r) => r.type === 'searchReplaceExecute')).toEqual({
      type: 'searchReplaceExecute',
      scope: { type: 'chat', chatId: 'chat-1' },
      searchText: 'lighthouse',
      replaceText: '',
      includeMessages: true,
      includeMemories: true,
    });
    expect(fixture.nativeElement.textContent).toContain('Operation Complete');
    expect(fixture.nativeElement.textContent).toContain('Successfully updated 4 items');
    expect(fixture.componentInstance.completions).toBe(1);
  });

  it('lands on the results step even when the execute fails', async () => {
    executeAnswer = new Error('the ledger would not take it');
    const fixture = await render();
    await typeSearch(fixture, 'lighthouse');
    button(fixture, 'Next').click();
    fixture.detectChanges();
    const tick = fixture.nativeElement.querySelector(
      'input[aria-label="I understand"]',
    ) as HTMLInputElement;
    tick.checked = true;
    tick.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    button(fixture, 'Replace All').click();
    await settle(fixture);

    expect(fixture.nativeElement.textContent).toContain('Operation Failed');
    expect(fixture.nativeElement.textContent).toContain('the ledger would not take it');
    expect(fixture.componentInstance.completions).toBe(0);
    // The only way out is Close, which is what v4 offers on results.
    expect(button(fixture, 'Close')).toBeTruthy();
  });

  it('switches the scope to a character and sends that shape instead', async () => {
    const fixture = await render();
    button(fixture, 'Back').click();
    fixture.detectChanges();
    const radio = fixture.nativeElement.querySelector(
      'input[aria-label="All Chats for Character"]',
    ) as HTMLInputElement;
    radio.dispatchEvent(new Event('change'));
    await settle(fixture);

    // Nothing chosen yet, so there is no scope and Next is dead.
    expect(button(fixture, 'Next').disabled).toBe(true);
    const picker = fixture.nativeElement.querySelector(
      'select[aria-label="Select a character"]',
    ) as HTMLSelectElement;
    picker.value = 'c1';
    picker.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(button(fixture, 'Next').disabled).toBe(false);

    button(fixture, 'Next').click();
    await settle(fixture);
    await typeSearch(fixture, 'lighthouse');
    expect(sent.find((r) => r.type === 'searchReplacePreview')!['scope']).toEqual({
      type: 'character',
      characterId: 'c1',
    });
  });

  it('surfaces a preview failure without blocking the box', async () => {
    previewAnswer = new Error('the index is out');
    const fixture = await render();
    await typeSearch(fixture, 'lighthouse');
    expect(fixture.nativeElement.textContent).toContain('the index is out');
    expect(button(fixture, 'Next').disabled).toBe(true);
  });
});
