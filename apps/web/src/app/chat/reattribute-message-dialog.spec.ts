import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import type { ParticipantDetail } from '../core/core-contract';
import { ReattributeMessageDialog } from './reattribute-message-dialog';

/** Re-attribute Message (v4 `ReattributeMessageDialog.tsx`). */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let answer: Record<string, unknown> | Error = { memoriesDeleted: 2 };

function stubClient(): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      sent.push(req);
      if (answer instanceof Error) throw answer;
      return answer;
    }) as unknown as CoreClient['dispatchData'],
  };
}

function participant(id: string, name: string, title: string | null = null): ParticipantDetail {
  return {
    id,
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: { id: `char-${id}`, name, title, avatarUrl: null, defaultImageId: null, defaultImage: null },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '',
    updatedAt: '',
  } as ParticipantDetail;
}

@Component({
  imports: [ReattributeMessageDialog],
  template: `
    <qt-reattribute-message-dialog
      [messageId]="'msg-1'"
      [currentParticipantId]="current()"
      [participants]="participants()"
      (reattributed)="emitted = emitted + 1"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  readonly current = signal<string | null>('p1');
  readonly participants = signal<ParticipantDetail[]>([
    participant('p1', 'Lorian'),
    participant('p2', 'Riya', 'Quartermaster'),
  ]);
  emitted = 0;
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

function options(fixture: ComponentFixture<Host>): HTMLButtonElement[] {
  return Array.from(
    fixture.nativeElement.querySelectorAll('.qt-dialog-body button'),
  ) as HTMLButtonElement[];
}

function submit(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.trim().startsWith('Re-attribut'),
  )!;
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ReattributeMessageDialog', () => {
  beforeEach(() => {
    sent.length = 0;
    answer = { memoriesDeleted: 2 };
  });

  it('excludes the message’s current author and preselects nobody', async () => {
    const fixture = await render();
    const rows = options(fixture);
    expect(rows).toHaveLength(1);
    // The avatar contributes the initial, so match on substrings rather than
    // pinning the concatenation.
    expect(rows[0].textContent).toContain('Riya');
    expect(rows[0].textContent).toContain('Character');
    expect(rows[0].textContent).toContain('Quartermaster');
    expect(submit(fixture).disabled).toBe(true);
  });

  it('sends the chosen participant and reports the memories deleted', async () => {
    const fixture = await render();
    options(fixture)[0].click();
    fixture.detectChanges();
    expect(submit(fixture).disabled).toBe(false);

    submit(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sent).toEqual([
      { type: 'messageReattribute', messageId: 'msg-1', newParticipantId: 'p2' },
    ]);
    expect(fixture.componentInstance.emitted).toBe(1);
    expect(toasts()).toEqual([
      { type: 'success', message: 'Message re-attributed to Riya. 2 memories deleted.' },
    ]);
    expect(fixture.componentInstance.closed).toBe(1);
  });

  it('drops the memories half of the sentence at zero, and singularises at one', async () => {
    answer = { memoriesDeleted: 0 };
    const fixture = await render();
    options(fixture)[0].click();
    fixture.detectChanges();
    submit(fixture).click();
    await fixture.whenStable();
    expect(toasts()).toEqual([{ type: 'success', message: 'Message re-attributed to Riya.' }]);

    answer = { memoriesDeleted: 1 };
    const second = await render();
    options(second)[0].click();
    second.detectChanges();
    submit(second).click();
    await second.whenStable();
    expect(toasts()).toEqual([
      { type: 'success', message: 'Message re-attributed to Riya. 1 memory deleted.' },
    ]);
  });

  it('shows v4’s empty state when the cast holds nobody else', async () => {
    const fixture = await render();
    fixture.componentInstance.participants.set([participant('p1', 'Lorian')]);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('No other participants available');
    expect(submit(fixture).disabled).toBe(true);
  });

  it('reports a failure and stays open', async () => {
    answer = new Error('the clerk refuses');
    const fixture = await render();
    options(fixture)[0].click();
    fixture.detectChanges();
    submit(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'the clerk refuses' }]);
    expect(fixture.componentInstance.closed).toBe(0);
  });
});

describe('ReattributeMessageDialog — WCAG semantics (P4.D358)', () => {
  function escape(): void {
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
    );
  }

  it('announces as a dialog named by its title', async () => {
    const fixture = await render();
    const dlg = fixture.nativeElement.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect(
      (fixture.nativeElement.querySelector('#' + id) as HTMLElement).textContent?.trim(),
    ).toBe('Re-attribute Message');
  });

  it('a document Escape closes once — but not while a re-attribution is in flight', async () => {
    const fixture = await render();
    const dialog = fixture.debugElement.query(By.directive(ReattributeMessageDialog))
      .componentInstance as unknown as { submitting: { set(v: boolean): void } };
    dialog.submitting.set(true);
    escape();
    expect(fixture.componentInstance.closed).toBe(0);
    dialog.submitting.set(false);
    escape();
    expect(fixture.componentInstance.closed).toBe(1);
  });
});
