import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { RichEditor } from '../editor/rich-editor';
import { ToastService } from '../ui/toast.service';
import { InformDialog, type InformAudienceCandidate } from './inform-dialog';

/**
 * Parity spec against v4 `components/chat/InformDialog.tsx` and its own
 * `__tests__/unit/components/chat/InformDialog.test.tsx` at `f45a517a9`
 * (`e7d77bb60`). Nine cases, in v4's order and under v4's own names:
 *
 *   audience — offers every LLM seat and never the user-controlled one;
 *              shows a silent seat with its status and still lets it be picked;
 *              starts on Everyone and drops it the moment a seat is picked;
 *              says so when no seat is played by a model;
 *   posting  — disables Inform until the passage has a body;
 *              posts null targets for Everyone, then closes and reports;
 *              posts the chosen ids for a subset;
 *              collapses a full hand-picked selection back to null;
 *              stays open and does not report when the post fails.
 *
 * v4's fixture ids are carried verbatim, so a diff against its file reads
 * straight across. The transport differs by construction: v4's dialog `fetch`es
 * `?action=inform` and its spec reads the POST body; v5's dispatches
 * `chatInform` and this spec reads the dispatched request — the SAME three keys
 * either way (§S.1), which is the thing the cases are actually about.
 */

const ALICE = 'aaaa1111-1111-1111-1111-111111111111';
const BOB = 'bbbb2222-2222-2222-2222-222222222222';
const PLAYER = 'cccc3333-3333-3333-3333-333333333333';

const CANDIDATES: InformAudienceCandidate[] = [
  { participantId: ALICE, name: 'Alice', controlledBy: 'llm', status: 'active' },
  { participantId: BOB, name: 'Bob', controlledBy: 'llm', status: 'silent' },
  { participantId: PLAYER, name: 'The Operator', controlledBy: 'user', status: 'active' },
];

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

/** `dispatchData` answers `chatInform`; pass an Error to exercise the failure arm. */
function stub(result?: Record<string, unknown> | Error): Stub {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    if (req['type'] === 'chatInform') {
      if (result instanceof Error) throw result;
      return (
        result ?? { success: true, batchId: 'batch-1', targetParticipantIds: null, message: null }
      );
    }
    return {};
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(
  s: Stub,
  audienceCandidates: InformAudienceCandidate[] = CANDIDATES,
): Promise<ComponentFixture<InformDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [InformDialog],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: s.client },
    ],
  });
  const fixture = TestBed.createComponent(InformDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('audienceCandidates', audienceCandidates);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/** Every button in the dialog, by accessible name (v4's `getByRole`). */
function buttons(fixture: ComponentFixture<unknown>): HTMLButtonElement[] {
  return [...fixture.nativeElement.querySelectorAll('button')] as HTMLButtonElement[];
}

function seat(fixture: ComponentFixture<unknown>, needle: string): HTMLButtonElement | undefined {
  return buttons(fixture).find((b) => (b.textContent ?? '').includes(needle));
}

function named(fixture: ComponentFixture<unknown>, exact: string): HTMLButtonElement {
  return buttons(fixture).find((b) => (b.textContent ?? '').trim() === exact)!;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
}

/**
 * Drive the passage through the real `qt-markdown-field` → `RichEditor` chain
 * (the `insert-announcement-dialog.spec.ts` idiom). The settle is load-bearing:
 * `RichEditor.replaceContent` defers its emit by a microtask, so a synchronous
 * click straight after would read the pre-edit state.
 */
async function setPassage(fixture: ComponentFixture<InformDialog>, value: string): Promise<void> {
  const editor = fixture.debugElement.query(By.directive(RichEditor));
  (editor.componentInstance as RichEditor).setMarkdown(value);
  await settle(fixture);
}

/** The body of the most recent `chatInform` dispatch, v4's `postedBody`. */
function postedBody(s: Stub): Record<string, unknown> {
  const informs = s.calls.filter((c) => c['type'] === 'chatInform');
  const last = informs[informs.length - 1]!;
  const { type: _type, ...body } = last;
  return body;
}

describe('InformDialog — audience (v4 components/chat/InformDialog.tsx @ f45a517a9)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('offers every LLM seat and never the user-controlled one', async () => {
    const fixture = await mount(stub());
    expect(seat(fixture, 'Alice')).toBeTruthy();
    expect(seat(fixture, 'Bob')).toBeTruthy();
    expect(seat(fixture, 'The Operator')).toBeUndefined();
  });

  it('shows a silent seat with its status and still lets it be picked', async () => {
    const fixture = await mount(stub());
    const bob = seat(fixture, 'Bob')!;
    expect(bob.textContent).toContain('(silent)');
    bob.click();
    fixture.detectChanges();
    expect(seat(fixture, 'Bob')!.getAttribute('aria-pressed')).toBe('true');
  });

  it('starts on Everyone and drops it the moment a seat is picked', async () => {
    const fixture = await mount(stub());
    expect(named(fixture, 'Everyone').getAttribute('aria-pressed')).toBe('true');

    seat(fixture, 'Alice')!.click();
    fixture.detectChanges();
    expect(named(fixture, 'Everyone').getAttribute('aria-pressed')).toBe('false');

    named(fixture, 'Everyone').click();
    fixture.detectChanges();
    expect(seat(fixture, 'Alice')!.getAttribute('aria-pressed')).toBe('false');
  });

  it('says so when no seat is played by a model', async () => {
    const fixture = await mount(stub(), [CANDIDATES[2]!]);
    expect(text(fixture)).toContain('nobody to take the note aside');
    expect(named(fixture, 'Inform').disabled).toBe(true);
  });
});

describe('InformDialog — posting (v4 components/chat/InformDialog.tsx @ f45a517a9)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('disables Inform until the passage has a body', async () => {
    const fixture = await mount(stub());
    expect(named(fixture, 'Inform').disabled).toBe(true);

    await setPassage(fixture, 'You notice the clock has stopped.');
    expect(named(fixture, 'Inform').disabled).toBe(false);
  });

  it('posts null targets for Everyone, then closes and reports', async () => {
    const s = stub();
    const fixture = await mount(s);
    const closed = vi.fn();
    const posted = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    fixture.componentInstance.posted.subscribe(posted);

    await setPassage(fixture, 'You notice the clock has stopped.');
    named(fixture, 'Inform').click();
    await settle(fixture);

    expect(closed).toHaveBeenCalled();
    expect(postedBody(s)).toEqual({
      chatId: 'chat-1',
      contentMarkdown: 'You notice the clock has stopped.',
      targetParticipantIds: null,
    });
    expect(posted).toHaveBeenCalledTimes(1);
    expect(TestBed.inject(ToastService).toasts().map((t) => t.message)).toContain(
      'The company has been informed',
    );
  });

  it('posts the chosen ids for a subset', async () => {
    const s = stub();
    const fixture = await mount(s);
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);

    seat(fixture, 'Alice')!.click();
    fixture.detectChanges();
    await setPassage(fixture, 'You see Bob pocket the key.');
    named(fixture, 'Inform').click();
    await settle(fixture);

    expect(closed).toHaveBeenCalled();
    expect(postedBody(s)).toEqual({
      chatId: 'chat-1',
      contentMarkdown: 'You see Bob pocket the key.',
      targetParticipantIds: [ALICE],
    });
    // v4's other half of the subset arm, its success toast.
    expect(TestBed.inject(ToastService).toasts().map((t) => t.message)).toContain('Informed Alice');
  });

  it('collapses a full hand-picked selection back to null', async () => {
    const s = stub();
    const fixture = await mount(s);
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);

    seat(fixture, 'Alice')!.click();
    fixture.detectChanges();
    seat(fixture, 'Bob')!.click();
    fixture.detectChanges();
    await setPassage(fixture, 'You hear the gate close.');
    named(fixture, 'Inform').click();
    await settle(fixture);

    expect(closed).toHaveBeenCalled();
    // Assert the whole body, so this can only be reading its own dispatch.
    expect(postedBody(s)).toEqual({
      chatId: 'chat-1',
      contentMarkdown: 'You hear the gate close.',
      targetParticipantIds: null,
    });
  });

  it('stays open and does not report when the post fails', async () => {
    const s = stub(new Error('No LLM-controlled seat to inform.'));
    const fixture = await mount(s);
    const closed = vi.fn();
    const posted = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    fixture.componentInstance.posted.subscribe(posted);

    await setPassage(fixture, 'You notice nothing at all.');
    named(fixture, 'Inform').click();
    await settle(fixture);

    expect(named(fixture, 'Inform').disabled).toBe(false);
    expect(posted).not.toHaveBeenCalled();
    expect(closed).not.toHaveBeenCalled();
    expect(TestBed.inject(ToastService).toasts().map((t) => t.message)).toContain(
      'No LLM-controlled seat to inform.',
    );
  });
});
