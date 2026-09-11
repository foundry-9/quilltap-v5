import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { ImpersonationVoiceDialog } from './impersonation-voice-dialog';

/**
 * The In Their Own Words review dialog (v4
 * `components/chat/ImpersonationVoiceDialog.tsx`) — every string, control,
 * disabled rule and order in §A, plus the two rules the feature exists for: the
 * two doors that survive a failed preview, and Cmd/Ctrl+Enter.
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
): Promise<ComponentFixture<ImpersonationVoiceDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ImpersonationVoiceDialog],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(ImpersonationVoiceDialog);
  fixture.componentRef.setInput('characterName', 'Evangeline');
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
    const f = await mount({ profileName: 'Her own desk' });
    // Read the line itself: the profile picker's own options carry em dashes.
    const line = Array.from((f.nativeElement as HTMLElement).querySelectorAll('.qt-text-xs')).find(
      (el) => (el.textContent ?? '').includes('Spoken through'),
    );
    expect(line?.textContent?.trim()).toBe('Spoken through Her own desk');
  });

  it('shows no voice line at all when neither is known', async () => {
    const f = await mount();
    expect((f.nativeElement as HTMLElement).textContent).not.toContain('Spoken through');
  });

  it('falls back to the initial when there is no portrait', async () => {
    const f = await mount();
    const el = f.nativeElement as HTMLElement;
    expect(el.querySelector('img')).toBeNull();
    expect(el.querySelector('.qt-bg-secondary span')?.textContent).toBe('E');
  });
});

describe('ImpersonationVoiceDialog — the five doors, in v4’s order', () => {
  it('carries exactly v4’s five buttons, in v4’s order', async () => {
    const f = await mount({ proposal: 'a proposal' });
    expect(buttons(f).map((b) => (b.textContent ?? '').trim())).toEqual([
      'Cancel',
      'Edit original',
      'Send as written',
      'Regenerate',
      'Send',
    ]);
  });

  it('disables every door while a preview is in flight, and relabels Send', async () => {
    const f = await mount({ generating: true, seed: 'a draft', proposal: 'stale' });
    expect(buttons(f).map((b) => b.disabled)).toEqual([true, true, true, true, true]);
    expect(button(f, 'Rehearsing…')).toBeTruthy();
  });

  it('a FAILED preview leaves Send as written and Edit original reachable', async () => {
    // The failure lands as `generating: false` with an empty proposal — a dead
    // provider must never trap a draft behind a dialog with nothing to press.
    const f = await mount({ generating: false, proposal: '', seed: 'my own words' });
    expect(button(f, 'Send as written').disabled).toBe(false);
    expect(button(f, 'Edit original').disabled).toBe(false);
    expect(button(f, 'Cancel').disabled).toBe(false);
    // …and Send is refused, because there is nothing to send.
    expect(button(f, 'Send').disabled).toBe(true);
  });

  it('names the empty proposal out loud', async () => {
    const f = await mount({ generating: false, proposal: '   ' });
    expect((f.nativeElement as HTMLElement).textContent).toContain(
      'Nothing came back. Send your own words as written, or go back and rewrite them.',
    );
  });

  it('says nothing about an empty proposal while one is still in flight', async () => {
    const f = await mount({ generating: true, proposal: '' });
    expect((f.nativeElement as HTMLElement).textContent).not.toContain('Nothing came back.');
  });

  it('Regenerate is refused on a blank draft', async () => {
    const f = await mount({ seed: '   ', proposal: 'x' });
    expect(button(f, 'Regenerate').disabled).toBe(true);
  });

  it('Send emits the proposal', async () => {
    const f = await mount({ proposal: 'I shall take the position.' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    button(f, 'Send').click();
    expect(seen).toEqual(['I shall take the position.']);
  });

  it('the other four doors emit their own events', async () => {
    const f = await mount({ seed: 'a draft', proposal: 'a proposal' });
    const seen: string[] = [];
    f.componentInstance.cancel.subscribe(() => seen.push('cancel'));
    f.componentInstance.editOriginal.subscribe(() => seen.push('edit'));
    f.componentInstance.sendAsWritten.subscribe(() => seen.push('asWritten'));
    f.componentInstance.regenerate.subscribe(() => seen.push('regenerate'));
    button(f, 'Cancel').click();
    button(f, 'Edit original').click();
    button(f, 'Send as written').click();
    button(f, 'Regenerate').click();
    expect(seen).toEqual(['cancel', 'edit', 'asWritten', 'regenerate']);
  });
});

describe('ImpersonationVoiceDialog — Cmd/Ctrl+Enter', () => {
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
    const f = await mount({ proposal: 'ready' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    expect(press(f, { metaKey: true })).toBe(true);
    expect(seen).toEqual(['ready']);
  });

  it('sends on Ctrl+Enter', async () => {
    const f = await mount({ proposal: 'ready' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    press(f, { ctrlKey: true });
    expect(seen).toEqual(['ready']);
  });

  it('a bare Enter never sends', async () => {
    const f = await mount({ proposal: 'ready' });
    const seen: string[] = [];
    f.componentInstance.send.subscribe((v) => seen.push(v));
    expect(press(f, {})).toBe(false);
    expect(seen).toEqual([]);
  });

  it('does not send when Send itself is refused', async () => {
    const f = await mount({ proposal: '   ' });
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
    // options do not exist on the first render.
    const f = await mount({ profileOverride: 'p-2' });
    expect(select(f, 'impersonation-voice-profile')!.value).toBe('p-2');
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
      generating: true,
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
    const f = await mount({ proposal: 'a proposal' });
    const el = f.nativeElement as HTMLElement;
    expect(el.textContent).toContain('What Evangeline will say');
    expect(el.querySelector('[aria-label="What Evangeline will say"]')).not.toBeNull();
  });

  it('shows the quill in place of the proposal while generating', async () => {
    const f = await mount({ generating: true });
    const el = f.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Generating in character…');
    expect(el.querySelector('[aria-label="What Evangeline will say"]')).toBeNull();
  });

  it('the backdrop cannot dismiss a dialog mid-preview', async () => {
    const f = await mount({ generating: true });
    const seen: string[] = [];
    f.componentInstance.cancel.subscribe(() => seen.push('cancel'));
    (f.nativeElement as HTMLElement).querySelector<HTMLElement>('.qt-dialog-overlay')!.click();
    expect(seen).toEqual([]);
  });

  it('…but does dismiss one at rest, as Cancel would', async () => {
    const f = await mount({ proposal: 'a proposal' });
    const seen: string[] = [];
    f.componentInstance.cancel.subscribe(() => seen.push('cancel'));
    (f.nativeElement as HTMLElement).querySelector<HTMLElement>('.qt-dialog-overlay')!.click();
    expect(seen).toEqual(['cancel']);
  });
});
