import { signal, type Provider } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { IMAGES_HIDDEN } from '../hidden-image/images-hidden';
import { ImpersonationVoiceDialog } from './impersonation-voice-dialog';

/**
 * The In Their Own Words review dialog (v4
 * `components/chat/ImpersonationVoiceDialog.tsx`) — every string, control,
 * disabled rule and order in §A, plus the two rules the feature exists for: the
 * two doors that survive a failed preview, and Cmd/Ctrl+Enter.
 *
 * Since v4 `07b8f0209` the dialog has a `stage` (`draft` / `generating` /
 * `review`) in place of a `generating` flag, and two footers. `mount` defaults
 * to `draft`, as v4's own `renderDialog` does; the review-state blocks pass
 * `stage: 'review'`. The draft-state block is transcribed from v4's NEW
 * `__tests__/unit/components/chat/ImpersonationVoiceDialog.test.tsx`.
 */

const PROFILES = {
  profiles: [
    {
      id: 'p-1',
      name: 'The house desk',
      provider: 'OPENAI',
      modelName: 'gpt-house',
      isDefault: true,
    },
    {
      id: 'p-2',
      name: 'A borrowed voice',
      provider: 'GROK',
      modelName: 'grok-2',
      isDefault: false,
    },
  ],
};

function stub(profilesAnswer: () => Promise<Record<string, unknown>> = async () => PROFILES) {
  return {
    dispatchData: vi.fn(async () => profilesAnswer()),
  } as unknown as CoreClient;
}

async function mount(
  over: Record<string, unknown> = {},
  client: CoreClient = stub(),
  extra: Provider[] = [],
): Promise<ComponentFixture<ImpersonationVoiceDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ImpersonationVoiceDialog],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
      ...extra,
    ],
  });
  const fixture = TestBed.createComponent(ImpersonationVoiceDialog);
  fixture.componentRef.setInput('characterName', 'Evangeline');
  fixture.componentRef.setInput('stage', 'draft');
  for (const [k, v] of Object.entries(over)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function buttons(f: ComponentFixture<unknown>): HTMLButtonElement[] {
  return Array.from((f.nativeElement as HTMLElement).querySelectorAll('[qt-modal-footer] button'));
}

function button(f: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  const found = buttons(f).find((b) => (b.textContent ?? '').trim() === label);
  if (!found) throw new Error(`no footer button labelled ${label}`);
  return found;
}

afterEach(() => TestBed.resetTestingModule());

describe('ImpersonationVoiceDialog — the frame', () => {
  it("titles itself in the character's own words", async () => {
    const f = await mount();
    expect((f.nativeElement as HTMLElement).querySelector('.qt-dialog-title')?.textContent).toBe(
      "In Evangeline's own words",
    );
  });

  it('names the seat, its title, and the voice it was spoken through', async () => {
    const f = await mount({
      stage: 'review',
      characterTitle: 'the Aeronaut',
      profileName: 'Her own desk',
      modelName: 'gpt-seat',
    });
    const text = (f.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Evangeline');
    expect(text).toContain('the Aeronaut');
    expect(text).toContain('Spoken through Her own desk — gpt-seat');
  });

  it('drops the model half when only the profile is known', async () => {
    const f = await mount({ stage: 'review', profileName: 'Her own desk' });
    // Read the line itself: the profile picker's own options carry em dashes.
    const line = Array.from((f.nativeElement as HTMLElement).querySelectorAll('.qt-text-xs')).find(
      (el) => (el.textContent ?? '').includes('Spoken through'),
    );
    expect(line?.textContent?.trim()).toBe('Spoken through Her own desk');
  });

  it('shows no voice line at all when neither is known', async () => {
    const f = await mount({ stage: 'review' });
    expect((f.nativeElement as HTMLElement).textContent).not.toContain('Spoken through');
    const g = await mount();
    expect((g.nativeElement as HTMLElement).textContent).not.toContain('spoken through');
  });

  /**
   * v4 `ImpersonationVoiceDialog.tsx:135-144`: before a restatement the line
   * names the voice the PICKER has chosen ("Would be spoken through"); after
   * one, the voice the server actually used.
   */
  it('in the draft, says "Would be spoken through" the seat’s own voice', async () => {
    const f = await mount({ profileName: 'Her own desk', modelName: 'gpt-seat' });
    expect((f.nativeElement as HTMLElement).textContent).toContain(
      'Would be spoken through Her own desk — gpt-seat',
    );
  });

  it('in the draft, names the PICKED profile and its model over the seat’s own', async () => {
    const f = await mount({
      profileName: 'Her own desk',
      modelName: 'gpt-seat',
      profileOverride: 'p-2',
    });
    expect((f.nativeElement as HTMLElement).textContent).toContain(
      'Would be spoken through A borrowed voice — grok-2',
    );
  });

  it('in review, the picked profile is NOT read — the server’s answer is', async () => {
    const f = await mount({
      stage: 'review',
      proposal: 'x',
      profileName: 'Her own desk',
      modelName: 'gpt-seat',
      profileOverride: 'p-2',
    });
    const text = (f.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Spoken through Her own desk — gpt-seat');
    expect(text).not.toContain('Would be spoken through');
  });

  it('the verb follows the stage while generating too ("Spoken through")', async () => {
    const f = await mount({ stage: 'generating', profileName: 'Her own desk' });
    const text = (f.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Spoken through Her own desk');
    expect(text).not.toContain('Would be spoken through');
  });

  it('falls back to the initial when there is no portrait', async () => {
    const f = await mount();
    const el = f.nativeElement as HTMLElement;
    expect(el.querySelector('img')).toBeNull();
    expect(el.querySelector('.qt-bg-secondary span')?.textContent).toBe('E');
  });
});

/**
 * The draft state — nothing has been sent to a model. Transcribed case for case
 * from v4 `ImpersonationVoiceDialog.test.tsx:76-122` (`07b8f0209`), with v4's
 * names; v4 finds buttons by role + accessible name, v5 by the footer's
 * trimmed text (the same thing for a text-only button).
 */
describe('ImpersonationVoiceDialog — the draft state (v4 ImpersonationVoiceDialog.test.tsx)', () => {
  function seedBox(f: ComponentFixture<ImpersonationVoiceDialog>): HTMLElement {
    // v4 fires keyDown on the editor (`getByLabelText('Your draft')`) and lets
    // it bubble to the wrapper's `onKeyDown`; v5 does the same from the field.
    return (f.nativeElement as HTMLElement).querySelector('[aria-label="Your draft"]')!;
  }
  function pressIn(el: HTMLElement, init: KeyboardEventInit): boolean {
    const ev = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
      ...init,
    });
    el.dispatchEvent(ev);
    return ev.defaultPrevented;
  }

  it('offers Send as written and Restate, and neither Send nor Regenerate', async () => {
    const f = await mount({ seed: 'I tell him I will take the job.' });
    const labels = buttons(f).map((b) => (b.textContent ?? '').trim());
    expect(labels).toContain('Send as written');
    expect(labels).toContain('Restate in their voice');
    expect(labels).not.toContain('Send');
    expect(labels).not.toContain('Regenerate');
    expect(
      (f.nativeElement as HTMLElement).querySelector('[aria-label="What Evangeline will say"]'),
    ).toBeNull();
  });

  it('makes Send as written the primary action', async () => {
    const f = await mount({ seed: 'I tell him I will take the job.' });
    expect(button(f, 'Send as written').className).toContain('qt-button-primary');
    expect(button(f, 'Restate in their voice').className).toContain('qt-button-secondary');
  });

  it('sends as written on click', async () => {
    const f = await mount({ seed: 'I tell him I will take the job.' });
    const seen: string[] = [];
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    f.componentInstance.restate.subscribe(() => seen.push('restate'));
    button(f, 'Send as written').click();
    expect(seen).toEqual(['asWritten']);
  });

  it('asks for a restatement only when Restate is pressed', async () => {
    const f = await mount({ seed: 'I tell him I will take the job.' });
    const seen: string[] = [];
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    f.componentInstance.restate.subscribe(() => seen.push('restate'));
    button(f, 'Restate in their voice').click();
    expect(seen).toEqual(['restate']);
  });

  it('sends as written on Cmd/Ctrl+Enter in the draft', async () => {
    const f = await mount({ seed: 'I tell him I will take the job.' });
    const seen: string[] = [];
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    f.componentInstance.restate.subscribe(() => seen.push('restate'));
    expect(pressIn(seedBox(f), { metaKey: true })).toBe(true);
    expect(seen).toEqual(['asWritten']);
    pressIn(seedBox(f), { ctrlKey: true });
    expect(seen).toEqual(['asWritten', 'asWritten']);
  });

  it('cannot send or restate an empty draft', async () => {
    const f = await mount({ seed: '   ' });
    expect(button(f, 'Send as written').disabled).toBe(true);
    expect(button(f, 'Restate in their voice').disabled).toBe(true);
    const seen: string[] = [];
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    pressIn(seedBox(f), { metaKey: true });
    expect(seen).toEqual([]);
  });

  // v5 additions over v4's block: the order, the bare Enter, the review arm of
  // the draft shortcut, and the note.
  it('carries v4’s four draft buttons, in v4’s order (`:291-326`)', async () => {
    const f = await mount({ seed: 'a draft' });
    expect(buttons(f).map((b) => (b.textContent ?? '').trim())).toEqual([
      'Cancel',
      'Edit original',
      'Restate in their voice',
      'Send as written',
    ]);
    expect(buttons(f).map((b) => b.disabled)).toEqual([false, false, false, false]);
  });

  it('a bare Enter in the draft never sends', async () => {
    const f = await mount({ seed: 'a draft' });
    const seen: string[] = [];
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    // (The editor itself consumes a bare Enter as a line break, so whether the
    // event was default-prevented says nothing about the dialog.)
    pressIn(seedBox(f), {});
    expect(seen).toEqual([]);
  });

  it('Cmd/Ctrl+Enter in the draft is refused while a restatement is in flight', async () => {
    const f = await mount({ stage: 'generating', seed: 'a draft' });
    const seen: string[] = [];
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    pressIn(seedBox(f), { metaKey: true });
    expect(seen).toEqual([]);
  });

  it('Cmd/Ctrl+Enter in the draft still sends as written in review (`!generating && hasDraft`)', async () => {
    const f = await mount({ stage: 'review', seed: 'a draft', proposal: 'a proposal' });
    const seen: string[] = [];
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    f.componentInstance.send.subscribe((v) => seen.push(`send:${v}`));
    pressIn(seedBox(f), { metaKey: true });
    expect(seen).toEqual(['asWritten']);
  });

  it('says what the two choices do, in v4’s words (`:264-270`)', async () => {
    const f = await mount({ seed: 'a draft' });
    const note = Array.from((f.nativeElement as HTMLElement).querySelectorAll('.qt-text-xs')).find(
      (el) => (el.textContent ?? '').includes('Sending as written'),
    );
    expect((note?.textContent ?? '').replace(/\s+/g, ' ').trim()).toBe(
      "Sending as written posts your words under Evangeline's name exactly as typed " +
        '(Cmd/Ctrl+Enter). Ask for a restatement only if you want Evangeline to put it in ' +
        'their own voice first.',
    );
    expect((f.nativeElement as HTMLElement).textContent).not.toContain('Nothing came back.');
  });

  it('the note is gone once a restatement is asked for', async () => {
    const f = await mount({ stage: 'review', seed: 'a draft', proposal: 'x' });
    expect((f.nativeElement as HTMLElement).textContent).not.toContain('Sending as written posts');
  });
});

describe('ImpersonationVoiceDialog — the review state, in v4’s order', () => {
  it('offers Send, Regenerate and Send as written (v4 `:124-131`)', async () => {
    const f = await mount({
      stage: 'review',
      seed: 'a draft',
      proposal: 'Very well. I shall take it.',
    });
    const labels = buttons(f).map((b) => (b.textContent ?? '').trim());
    expect(labels).toContain('Send');
    expect(labels).toContain('Regenerate');
    expect(labels).toContain('Send as written');
    expect(labels).not.toContain('Restate in their voice');
  });

  it('Send posts the proposal; Regenerate asks again (v4 `:133-139`)', async () => {
    const f = await mount({
      stage: 'review',
      seed: 'a draft',
      proposal: 'Very well. I shall take it.',
    });
    const sent: string[] = [];
    let restated = 0;
    f.componentInstance.send.subscribe((v) => sent.push(v));
    f.componentInstance.restate.subscribe(() => (restated += 1));
    button(f, 'Send').click();
    expect(sent).toEqual(['Very well. I shall take it.']);
    button(f, 'Regenerate').click();
    expect(restated).toBe(1);
  });

  it('keeps Send as written open after a failed restatement (v4 `:141-145`)', async () => {
    const f = await mount({
      stage: 'review',
      seed: 'I tell him I will take the job.',
      proposal: '',
    });
    expect(button(f, 'Send').disabled).toBe(true);
    expect(button(f, 'Send as written').disabled).toBe(false);
  });

  it('wears v4’s classes: only Send is primary', async () => {
    const f = await mount({ stage: 'review', seed: 'a draft', proposal: 'a proposal' });
    expect(buttons(f).map((b) => b.classList.contains('qt-button-primary'))).toEqual([
      false,
      false,
      false,
      false,
      true,
    ]);
  });

  it('carries exactly v4’s five buttons, in v4’s order', async () => {
    const f = await mount({ stage: 'review', proposal: 'a proposal' });
    expect(buttons(f).map((b) => (b.textContent ?? '').trim())).toEqual([
      'Cancel',
      'Edit original',
      'Send as written',
      'Regenerate',
      'Send',
    ]);
  });

  it('disables every door while a preview is in flight, and relabels Send', async () => {
    const f = await mount({ stage: 'generating', seed: 'a draft', proposal: 'stale' });
    expect(buttons(f).map((b) => b.disabled)).toEqual([true, true, true, true, true]);
    expect(button(f, 'Rehearsing…')).toBeTruthy();
  });

  it('a FAILED preview leaves Send as written and Edit original reachable', async () => {
    // The failure lands as `stage: 'review'` with an empty proposal — a dead
    // provider must never trap a draft behind a dialog with nothing to press.
    const f = await mount({ stage: 'review', proposal: '', seed: 'my own words' });
    expect(button(f, 'Send as written').disabled).toBe(false);
    expect(button(f, 'Edit original').disabled).toBe(false);
    expect(button(f, 'Cancel').disabled).toBe(false);
    // …and Send is refused, because there is nothing to send.
    expect(button(f, 'Send').disabled).toBe(true);
  });

  it('names the empty proposal out loud', async () => {
    const f = await mount({ stage: 'review', proposal: '   ' });
    expect((f.nativeElement as HTMLElement).textContent).toContain(
      'Nothing came back. Send your own words as written, or go back and rewrite them.',
    );
  });

  it('says nothing about an empty proposal while one is still in flight', async () => {
    const f = await mount({ stage: 'generating', proposal: '' });
    expect((f.nativeElement as HTMLElement).textContent).not.toContain('Nothing came back.');
  });

  it('Regenerate AND Send as written are refused on a blank draft (`generating || !hasDraft`)', async () => {
    const f = await mount({ stage: 'review', seed: '   ', proposal: 'x' });
    expect(button(f, 'Regenerate').disabled).toBe(true);
    expect(button(f, 'Send as written').disabled).toBe(true);
    expect(button(f, 'Send').disabled).toBe(false);
  });

  it('Send emits the proposal', async () => {
    const f = await mount({ stage: 'review', proposal: 'I shall take the position.' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    button(f, 'Send').click();
    expect(seen).toEqual(['I shall take the position.']);
  });

  it('the other four doors emit their own events', async () => {
    const f = await mount({ stage: 'review', seed: 'a draft', proposal: 'a proposal' });
    const seen: string[] = [];
    f.componentInstance.cancel.subscribe(() => seen.push('cancel'));
    f.componentInstance.editOriginal.subscribe(() => seen.push('edit'));
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    f.componentInstance.restate.subscribe(() => seen.push('restate'));
    button(f, 'Cancel').click();
    button(f, 'Edit original').click();
    button(f, 'Send as written').click();
    button(f, 'Regenerate').click();
    expect(seen).toEqual(['cancel', 'edit', 'asWritten', 'restate']);
  });
});

describe('ImpersonationVoiceDialog — Cmd/Ctrl+Enter in the proposal', () => {
  function press(f: ComponentFixture<ImpersonationVoiceDialog>, init: KeyboardEventInit): boolean {
    const box = (f.nativeElement as HTMLElement).querySelector(
      'qt-voice-rewrite-review-panel',
    )?.parentElement;
    const ev = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
      ...init,
    });
    box?.dispatchEvent(ev);
    return ev.defaultPrevented;
  }

  it('sends on Cmd+Enter', async () => {
    const f = await mount({ stage: 'review', proposal: 'ready' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    expect(press(f, { metaKey: true })).toBe(true);
    expect(seen).toEqual(['ready']);
  });

  it('sends on Ctrl+Enter', async () => {
    const f = await mount({ stage: 'review', proposal: 'ready' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    press(f, { ctrlKey: true });
    expect(seen).toEqual(['ready']);
  });

  it('a bare Enter never sends', async () => {
    const f = await mount({ stage: 'review', proposal: 'ready' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    expect(press(f, {})).toBe(false);
    expect(seen).toEqual([]);
  });

  it('does not send when Send itself is refused', async () => {
    const f = await mount({ stage: 'review', proposal: '   ' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    press(f, { metaKey: true });
    expect(seen).toEqual([]);
  });
});

describe('ImpersonationVoiceDialog — the pickers', () => {
  function select(f: ComponentFixture<unknown>, id: string): HTMLSelectElement | null {
    return (f.nativeElement as HTMLElement).querySelector(`#${id}`);
  }

  it('offers "Their own voice" first, named for the seat’s own profile', async () => {
    const f = await mount({ profileName: 'Her own desk' });
    const opts = Array.from(select(f, 'impersonation-voice-profile')!.options).map((o) => o.text);
    expect(opts[0]).toBe('Their own voice (Her own desk)');
  });

  it('drops the suffix when the seat has no profile of its own', async () => {
    const f = await mount();
    expect(select(f, 'impersonation-voice-profile')!.options[0].text).toBe('Their own voice');
  });

  it('lists every connection profile with its model and the default marker', async () => {
    const f = await mount();
    const opts = Array.from(select(f, 'impersonation-voice-profile')!.options).map((o) =>
      o.text.replace(/\s+/g, ' ').trim(),
    );
    expect(opts.slice(1)).toEqual([
      'The house desk — gpt-house (default)',
      'A borrowed voice — grok-2',
    ]);
  });

  it('emits the chosen id, and null for "their own voice"', async () => {
    const f = await mount();
    const seen: (string | null)[] = [];
    f.componentInstance.changeProfile.subscribe((v) => seen.push(v));
    const el = select(f, 'impersonation-voice-profile')!;
    el.value = 'p-2';
    el.dispatchEvent(new Event('change'));
    el.value = '';
    el.dispatchEvent(new Event('change'));
    expect(seen).toEqual(['p-2', null]);
  });

  it('re-applies the override on the render that fills the list (the controlled-select rule)', async () => {
    // A profile chosen by the SERVICE (a re-run started from Regenerate, say)
    // must show on the control; a naive one-time binding loses it because the
    // options do not exist on the first render. Run in BOTH stages: the draft
    // has no proposal panel, so the effect cannot lean on it for a document.
    const f = await mount({ profileOverride: 'p-2' });
    expect(select(f, 'impersonation-voice-profile')!.value).toBe('p-2');
    const g = await mount({ stage: 'review', proposal: 'x', profileOverride: 'p-2' });
    expect(select(g, 'impersonation-voice-profile')!.value).toBe('p-2');
  });

  // `removing-a-select-option-makes-a-spec-vacuous`: assert the select's
  // ABSENCE, not a value it could never hold.
  it('hides the prompt picker entirely at ≤ 1 prompts', async () => {
    expect(select(await mount(), 'impersonation-voice-prompt')).toBeNull();
    expect(
      select(
        await mount({ systemPrompts: [{ id: 's-1', name: 'Only one' }] }),
        'impersonation-voice-prompt',
      ),
    ).toBeNull();
  });

  it('shows it at 2+, seeded from the seat’s own selection', async () => {
    const f = await mount({
      systemPrompts: [
        { id: 's-1', name: 'Plain', isDefault: true },
        { id: 's-2', name: 'Florid' },
      ],
      selectedSystemPromptId: 's-2',
    });
    const el = select(f, 'impersonation-voice-prompt')!;
    expect(Array.from(el.options).map((o) => o.text.trim())).toEqual(['Plain (default)', 'Florid']);
    expect(el.value).toBe('s-2');
  });

  it('the override wins over the seat’s selection', async () => {
    const f = await mount({
      systemPrompts: [
        { id: 's-1', name: 'Plain' },
        { id: 's-2', name: 'Florid' },
      ],
      selectedSystemPromptId: 's-2',
      systemPromptOverride: 's-1',
    });
    expect(select(f, 'impersonation-voice-prompt')!.value).toBe('s-1');
  });

  it('both pickers are frozen while a preview is in flight', async () => {
    const f = await mount({
      stage: 'generating',
      systemPrompts: [
        { id: 's-1', name: 'Plain' },
        { id: 's-2', name: 'Florid' },
      ],
    });
    expect(select(f, 'impersonation-voice-profile')!.disabled).toBe(true);
    expect(select(f, 'impersonation-voice-prompt')!.disabled).toBe(true);
  });

  it('a failed profiles read toasts v4’s sentence and leaves the picker usable', async () => {
    const f = await mount(
      {},
      stub(async () => {
        throw new Error('HTTP 500');
      }),
    );
    expect(
      TestBed.inject(ToastService)
        .toasts()
        .map((t) => t.message),
    ).toEqual(['Failed to load connection profiles: HTTP 500']);
    // "Their own voice" is still there: the seat's own model can still speak.
    expect(select(f, 'impersonation-voice-profile')!.options).toHaveLength(1);
  });
});

describe('ImpersonationVoiceDialog — the draft and the proposal', () => {
  it('shows the draft under v4’s label, editable, and reports edits', async () => {
    const f = await mount({ seed: 'the line I typed' });
    const el = f.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Your draft');
    expect(el.querySelector('[aria-label="Your draft"]')).not.toBeNull();
    const seen: string[] = [];
    f.componentInstance.seedChange.subscribe((v) => seen.push(v));
    // The field's own contentChange is what the host listens to.
    const field = f.debugElement.query((n) => n.name === 'qt-markdown-field');
    field.triggerEventHandler('contentChange', 'an edited line');
    expect(seen).toEqual(['an edited line']);
  });

  it('labels the proposal panel for this character', async () => {
    const f = await mount({ stage: 'review', proposal: 'a proposal' });
    const el = f.nativeElement as HTMLElement;
    expect(el.textContent).toContain('What Evangeline will say');
    expect(el.querySelector('[aria-label="What Evangeline will say"]')).not.toBeNull();
  });

  it('shows the quill in place of the proposal while generating', async () => {
    const f = await mount({ stage: 'generating' });
    const el = f.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Generating in character…');
    expect(el.querySelector('[aria-label="What Evangeline will say"]')).toBeNull();
  });

  it('the backdrop cannot dismiss a dialog mid-preview', async () => {
    const f = await mount({ stage: 'generating' });
    const seen: string[] = [];
    f.componentInstance.cancel.subscribe(() => seen.push('cancel'));
    (f.nativeElement as HTMLElement).querySelector<HTMLElement>('.qt-dialog-overlay')!.click();
    expect(seen).toEqual([]);
  });

  it('…but does dismiss one at rest, as Cancel would', async () => {
    const f = await mount({ stage: 'review', proposal: 'a proposal' });
    const seen: string[] = [];
    f.componentInstance.cancel.subscribe(() => seen.push('cancel'));
    (f.nativeElement as HTMLElement).querySelector<HTMLElement>('.qt-dialog-overlay')!.click();
    expect(seen).toEqual(['cancel']);
  });
});

/**
 * Quick-hide "Salon Images" (v4 `e3937d7aa` `ImpersonationVoiceDialog.tsx:86,
 * :119`): the seat header's portrait falls to the `w-10 h-10` initial disc.
 * The dialog renders in `SalonConversation`'s own template (not portaled), so
 * the Salon's element-level provider reaches it — pinned here by providing the
 * token directly.
 */
describe('ImpersonationVoiceDialog — the Salon Images switch (v4 e3937d7aa)', () => {
  it('paints the portrait when images are shown', async () => {
    const f = await mount({ avatarUrl: '/img/evangeline.webp' }, stub(), [
      { provide: IMAGES_HIDDEN, useValue: signal(false) },
    ]);
    expect((f.nativeElement as HTMLElement).querySelector('img')?.getAttribute('src')).toBe(
      '/img/evangeline.webp',
    );
  });

  it('falls back to the initial disc while the Salon hides its images', async () => {
    const f = await mount({ avatarUrl: '/img/evangeline.webp' }, stub(), [
      { provide: IMAGES_HIDDEN, useValue: signal(true) },
    ]);
    const el = f.nativeElement as HTMLElement;
    expect(el.querySelector('img')).toBeNull();
    expect(el.querySelector('.qt-bg-secondary span')?.textContent).toBe('E');
  });
});
