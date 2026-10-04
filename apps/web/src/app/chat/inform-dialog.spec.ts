import { signal, type Provider } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { RichEditor } from '../editor/rich-editor';
import { ToastService } from '../ui/toast.service';
import { IMAGES_HIDDEN } from './hidden-image/images-hidden';
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
 * `chatInform` and this spec reads the dispatched request — the SAME keys either
 * way (§S.1), which is the thing the cases are actually about. Since v4
 * `52d6e7ecd` that is FOUR keys: each body gains `permanent: false` exactly as
 * v4's three updated assertions do (`InformDialog.test.tsx` `:157`, `:176`,
 * `:196` at the pin); the standing arms live in their own block below.
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
  extra: Provider[] = [],
): Promise<ComponentFixture<InformDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [InformDialog],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: s.client },
      ...extra,
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
      permanent: false,
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
      permanent: false,
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
      permanent: false,
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

/**
 * Standing informs (v4 `52d6e7ecd`). Every string below is read from v4's
 * `components/chat/InformDialog.tsx` at the pin:
 *
 *   the toggle's label `:297` and hint `:299-300` (id `:298`), the checkbox
 *   `:288-295` (default `false` `:102`, `disabled={isPosting}` `:292`); the
 *   guidance paragraph `:258-265` with its conditional tail `:263-265`; the
 *   posted body `:148-152` (`permanent` LAST); the success toast `:162-169`.
 *
 * The first case is v4's own new test (`InformDialog.test.tsx` `posts a
 * standing inform when the toggle is ticked (off by default)`), under its own
 * name; the rest pin the template and toast arms v4's suite leaves untested.
 */
describe('InformDialog — standing informs (v4 components/chat/InformDialog.tsx @ 52d6e7ecd)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const ONE_SHOT_TAIL =
    'It is never spoken aloud, and once they have had their turn it is gone, like a note fed to the fire.';
  const STANDING_TAIL =
    'It is never spoken aloud, and it stays at their elbow for every turn they take in this chat, until you withdraw it.';
  const GUIDANCE_HEAD =
    'Write it to them, in the second person, as something they now know or notice — ' +
    'You see that Alice slipped the letter into her sleeve. ' +
    'You remember that Bob and Carol were at school together. Everyone you tick receives ' +
    'the identical words before their next turn, so set down a passage that is true from ' +
    'each of their chairs. ';

  function toggle(fixture: ComponentFixture<unknown>): HTMLInputElement {
    const labels = [...fixture.nativeElement.querySelectorAll('label')] as HTMLLabelElement[];
    const label = labels.find((l) => (l.textContent ?? '').includes('Keep it standing in this chat'));
    return label!.querySelector('input[type="checkbox"]') as HTMLInputElement;
  }

  function tick(fixture: ComponentFixture<unknown>): void {
    toggle(fixture).click();
    fixture.detectChanges();
  }

  function guidance(fixture: ComponentFixture<unknown>): string {
    const div = fixture.nativeElement.querySelector('div.mb-4.qt-text-xs') as HTMLElement;
    return (div.textContent ?? '').replace(/\s+/g, ' ').trim();
  }

  function toasts(): string[] {
    return TestBed.inject(ToastService).toasts().map((t) => t.message);
  }

  it('posts a standing inform when the toggle is ticked (off by default)', async () => {
    const s = stub();
    const fixture = await mount(s);
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);

    expect(toggle(fixture).checked).toBe(false);
    tick(fixture);
    await setPassage(fixture, "You are the ship's cat.");
    named(fixture, 'Inform').click();
    await settle(fixture);

    expect(closed).toHaveBeenCalled();
    expect(postedBody(s)).toEqual({
      chatId: 'chat-1',
      contentMarkdown: "You are the ship's cat.",
      targetParticipantIds: null,
      permanent: true,
    });
    // v4's `JSON.stringify` body puts `permanent` LAST (`:148-152`).
    expect(Object.keys(postedBody(s))).toEqual([
      'chatId',
      'contentMarkdown',
      'targetParticipantIds',
      'permanent',
    ]);
  });

  it('labels the toggle and its hint, and wires the hint by aria-describedby', async () => {
    const fixture = await mount(stub());
    const box = toggle(fixture);
    expect(box.classList.contains('qt-checkbox')).toBe(true);
    expect(box.getAttribute('aria-describedby')).toBe('inform-permanent-hint');
    const hint = fixture.nativeElement.querySelector('#inform-permanent-hint') as HTMLElement;
    expect((hint.textContent ?? '').replace(/\s+/g, ' ').trim()).toBe(
      'Every turn they take here, until you withdraw it. This chat only — it follows no one anywhere else.',
    );
    // Last thing read before posting: the toggle sits FIRST in the footer row,
    // pushed left by `mr-auto` ahead of Cancel and Inform (v4 `:286-287`).
    const footer = box.closest('[qt-modal-footer]') as HTMLElement;
    expect(footer).toBeTruthy();
    const first = footer.firstElementChild as HTMLElement;
    expect(first.tagName).toBe('LABEL');
    expect(first.classList.contains('mr-auto')).toBe(true);
  });

  it('switches the guidance tail with the toggle', async () => {
    const fixture = await mount(stub());
    expect(guidance(fixture)).toBe(GUIDANCE_HEAD + ONE_SHOT_TAIL);
    tick(fixture);
    expect(guidance(fixture)).toBe(GUIDANCE_HEAD + STANDING_TAIL);
    tick(fixture);
    expect(guidance(fixture)).toBe(GUIDANCE_HEAD + ONE_SHOT_TAIL);
  });

  it('disables the toggle while the post is in flight', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const s = stub();
    const inner = s.client.dispatchData!;
    s.client.dispatchData = (async (req: Record<string, unknown>) => {
      await gate;
      return inner(req as never);
    }) as unknown as CoreClient['dispatchData'];
    const fixture = await mount(s);

    expect(toggle(fixture).disabled).toBe(false);
    await setPassage(fixture, 'You hear the gate close.');
    named(fixture, 'Inform').click();
    await settle(fixture);
    expect(toggle(fixture).disabled).toBe(true);

    release();
    await settle(fixture);
  });

  it('reports a standing note for the company when Everyone is told', async () => {
    const fixture = await mount(stub());
    tick(fixture);
    await setPassage(fixture, 'You notice the clock has stopped.');
    named(fixture, 'Inform').click();
    await settle(fixture);
    expect(toasts()).toContain('A standing note for the company, for the rest of this chat');
  });

  it('reports a standing note by name for a subset', async () => {
    const fixture = await mount(stub());
    seat(fixture, 'Alice')!.click();
    fixture.detectChanges();
    tick(fixture);
    await setPassage(fixture, 'You see Bob pocket the key.');
    named(fixture, 'Inform').click();
    await settle(fixture);
    expect(toasts()).toContain('A standing note for Alice, for the rest of this chat');
    expect(toasts()).not.toContain('Informed Alice');
  });

  it('opens unticked on a fresh mount even after a standing post', async () => {
    // v4 resets by conditional mount (`:105-107`); so does v5's salon. Pin that
    // the signal's initializer, not leftover state, decides the default.
    const first = await mount(stub());
    tick(first);
    expect(toggle(first).checked).toBe(true);
    const second = await mount(stub());
    expect(toggle(second).checked).toBe(false);
  });
});

/**
 * Quick-hide "Salon Images" (v4 `e3937d7aa` `InformDialog.tsx:91,:224`):
 * `p.avatarUrl && !imagesHidden` — a seat's portrait falls to the `w-5 h-5`
 * disc. Rendered in `SalonConversation`'s own template, so the Salon's
 * element-level provider reaches it.
 */
describe('InformDialog — the Salon Images switch (v4 e3937d7aa)', () => {
  const WITH_PORTRAIT: InformAudienceCandidate[] = [
    { participantId: ALICE, name: 'Alice', controlledBy: 'llm', status: 'active', avatarUrl: '/img/alice.webp' },
  ];

  it('paints the seat portrait when images are shown', async () => {
    const f = await mount(stub(), WITH_PORTRAIT, [{ provide: IMAGES_HIDDEN, useValue: signal(false) }]);
    expect(seat(f, 'Alice')?.querySelector('img')?.getAttribute('src')).toBe('/img/alice.webp');
  });

  it('falls back to the disc while the Salon hides its images', async () => {
    const f = await mount(stub(), WITH_PORTRAIT, [{ provide: IMAGES_HIDDEN, useValue: signal(true) }]);
    const button = seat(f, 'Alice')!;
    expect(button.querySelector('img')).toBeNull();
    expect(button.querySelector('div.w-5.h-5.rounded-full.qt-bg-secondary')).not.toBeNull();
  });
});
