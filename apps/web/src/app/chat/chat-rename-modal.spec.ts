import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import { ChatRenameModal } from './chat-rename-modal';

/**
 * Rename Chat (v4 `ChatRenameModal.tsx`). The load-bearing behaviour is the
 * checkbox's SIDE EFFECT — it is not a preference toggle, it is the only door in
 * either app to `regenerate-title`.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let regenerateAnswer: Record<string, unknown> | Error = { title: 'A Fresh Coat of Paint' };
let updateFails: string | null = null;

function stubClient(): Partial<CoreClient> {
  return {
    dispatch: (async (req: Req) => {
      sent.push(req);
      if (updateFails) {
        return { type: 'error', data: { message: updateFails } };
      }
      return { type: 'chat', data: { chat: {} } };
    }) as CoreClient['dispatch'],
    dispatchData: (async (req: Req) => {
      sent.push(req);
      if (regenerateAnswer instanceof Error) throw regenerateAnswer;
      return regenerateAnswer;
    }) as unknown as CoreClient['dispatchData'],
  };
}

@Component({
  imports: [ChatRenameModal],
  template: `
    <qt-chat-rename-modal
      [chatId]="'chat-1'"
      [currentTitle]="currentTitle()"
      [isManuallyRenamed]="isManuallyRenamed()"
      (renamed)="renamed.push($event)"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  readonly currentTitle = signal('An Evening at Sea');
  readonly isManuallyRenamed = signal(true);
  readonly renamed: { title: string; isManuallyRenamed: boolean }[] = [];
  closed = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [{ provide: CoreClient, useValue: stubClient() }],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

/** The toast stack this render raised, newest last (v4 answers all of these
 *  with `showSuccessToast`/`showErrorToast`). */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function titleField(fixture: ComponentFixture<Host>): HTMLInputElement {
  return fixture.nativeElement.querySelector('#chat-title');
}

function autoBox(fixture: ComponentFixture<Host>): HTMLInputElement {
  return fixture.nativeElement.querySelector('input[type="checkbox"]');
}

function saveButton(fixture: ComponentFixture<Host>): HTMLButtonElement | null {
  return (
    (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
      (b) => b.textContent!.trim() === 'Save' || b.textContent!.trim() === 'Saving...',
    ) ?? null
  );
}

async function check(fixture: ComponentFixture<Host>, on: boolean): Promise<void> {
  const box = autoBox(fixture);
  box.checked = on;
  box.dispatchEvent(new Event('change'));
  await fixture.whenStable();
  fixture.detectChanges();
}

describe('ChatRenameModal', () => {
  beforeEach(() => {
    sent.length = 0;
    updateFails = null;
    regenerateAnswer = { title: 'A Fresh Coat of Paint' };
  });

  it('seeds the field from the chat and reads the checkbox off isManuallyRenamed', async () => {
    const fixture = await render();
    expect(titleField(fixture).value).toBe('An Evening at Sea');
    // Manually renamed ⇒ automatic naming OFF ⇒ the field is editable and Save shows.
    expect(autoBox(fixture).checked).toBe(false);
    expect(titleField(fixture).disabled).toBe(false);
    expect(saveButton(fixture)).toBeTruthy();
  });

  it('hides Save and disables the field while automatic naming is on', async () => {
    const fixture = await render();
    fixture.componentInstance.isManuallyRenamed.set(false);
    fixture.detectChanges();
    expect(autoBox(fixture).checked).toBe(true);
    expect(titleField(fixture).disabled).toBe(true);
    expect(saveButton(fixture)).toBeNull();
  });

  it('regenerates the title the moment the box is CHECKED, then closes', async () => {
    const fixture = await render();
    await check(fixture, true);

    expect(sent).toEqual([{ type: 'chatRegenerateTitle', chatId: 'chat-1' }]);
    expect(fixture.componentInstance.renamed).toEqual([
      { title: 'A Fresh Coat of Paint', isManuallyRenamed: false },
    ]);
    expect(toasts()).toEqual([{ type: 'success', message: 'Title regenerated' }]);
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('fires NOTHING when the box is unchecked', async () => {
    const fixture = await render();
    fixture.componentInstance.isManuallyRenamed.set(false);
    fixture.detectChanges();
    await check(fixture, false);
    expect(sent).toEqual([]);
    expect(fixture.componentInstance.closed).toBe(0);
    // …and Save reappears, because there is now something to save.
    expect(saveButton(fixture)).toBeTruthy();
  });

  it('reverts the checkbox and stays open when regeneration fails', async () => {
    regenerateAnswer = new Error('the naming clerk is out to lunch');
    const fixture = await render();
    await check(fixture, true);

    expect(autoBox(fixture).checked).toBe(false);
    expect(fixture.componentInstance.closed).toBe(0);
    expect(toasts()).toEqual([{ type: 'error', message: 'the naming clerk is out to lunch' }]);
  });

  it('saves the trimmed title with the manual-rename flag', async () => {
    const fixture = await render();
    titleField(fixture).value = '  A Longer Evening  ';
    titleField(fixture).dispatchEvent(new Event('input'));
    fixture.detectChanges();
    saveButton(fixture)!.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([
      {
        type: 'chatUpdate',
        chatId: 'chat-1',
        chat: { title: 'A Longer Evening', isManuallyRenamed: true },
      },
    ]);
    expect(toasts()).toEqual([{ type: 'success', message: 'Chat renamed' }]);
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('refuses an empty title without writing', async () => {
    const fixture = await render();
    titleField(fixture).value = '   ';
    titleField(fixture).dispatchEvent(new Event('input'));
    fixture.detectChanges();
    // v4 disables Save on a blank field, so drive the guard through Enter, the
    // other path into `handleSave` (:131).
    titleField(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([]);
    expect(toasts()).toEqual([{ type: 'error', message: 'Title cannot be empty' }]);
  });

  it('closes without writing when neither the title nor the flag would move', async () => {
    const fixture = await render();
    saveButton(fixture)!.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([]);
    expect(fixture.componentInstance.closed).toBe(1);
    expect(fixture.componentInstance.renamed).toEqual([]);
  });

  it('writes when only the automatic-naming preference moved', async () => {
    // Same title, but the operator unticked automatic naming: v4's
    // `!useAutoRename === initialIsManuallyRenamed` guard must NOT swallow this.
    const fixture = await render();
    fixture.componentInstance.isManuallyRenamed.set(false);
    fixture.detectChanges();
    await check(fixture, false);
    saveButton(fixture)!.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([
      {
        type: 'chatUpdate',
        chatId: 'chat-1',
        chat: { title: 'An Evening at Sea', isManuallyRenamed: true },
      },
    ]);
  });

  it('reports a failed save and stays open', async () => {
    updateFails = 'the registry is closed';
    const fixture = await render();
    titleField(fixture).value = 'Something Else';
    titleField(fixture).dispatchEvent(new Event('input'));
    fixture.detectChanges();
    saveButton(fixture)!.click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'the registry is closed' }]);
    expect(fixture.componentInstance.closed).toBe(0);
  });

  it('Enter saves while the field is live and does nothing while it is not', async () => {
    const fixture = await render();
    titleField(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await fixture.whenStable();
    expect(fixture.componentInstance.closed).toBe(1);

    const second = await render();
    second.componentInstance.isManuallyRenamed.set(false);
    second.detectChanges();
    titleField(second).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await second.whenStable();
    expect(sent).toEqual([]);
    expect(second.componentInstance.closed).toBe(0);
  });
});
