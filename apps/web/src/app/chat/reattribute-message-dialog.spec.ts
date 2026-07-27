import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../core/core-client';
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
      (reattributed)="reported.push($event)"
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
  readonly reported: string[] = [];
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
    expect(fixture.componentInstance.reported).toEqual([
      'Message re-attributed to Riya. 2 memories deleted.',
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
    expect(fixture.componentInstance.reported).toEqual(['Message re-attributed to Riya.']);

    answer = { memoriesDeleted: 1 };
    const second = await render();
    options(second)[0].click();
    second.detectChanges();
    submit(second).click();
    await second.whenStable();
    expect(second.componentInstance.reported).toEqual([
      'Message re-attributed to Riya. 1 memory deleted.',
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
    expect(fixture.nativeElement.textContent).toContain('the clerk refuses');
    expect(fixture.componentInstance.closed).toBe(0);
  });
});
