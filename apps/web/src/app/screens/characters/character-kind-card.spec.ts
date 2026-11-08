// v5-authored from v4 `components/characters/CharacterKindPicker.tsx` +
// `app/aurora/shared/save-character-kind.ts` + the views' `handleSaveKind`
// (`b09ca46bb`, at the pin `624d0d25c`) — v4 ships the card without a test.
// `can-draw-diagrams-card.spec.ts` is the model.
import { Component, signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreDispatchError } from '../../core/core-contract';
import { CoreClient } from '../../core/core-client';
import { ToastService } from '../../ui/toast.service';
import { CharacterKindCard } from './character-kind-card';

function render(
  dispatchData: (req: unknown) => Promise<Record<string, unknown>>,
  inputs: Record<string, unknown> = {},
): { fixture: ComponentFixture<CharacterKindCard>; el: HTMLElement; client: QueryClient } {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  TestBed.configureTestingModule({
    imports: [CharacterKindCard],
    providers: [provideTanStackQuery(client), { provide: CoreClient, useValue: { dispatchData } }],
  });
  const fixture = TestBed.createComponent(CharacterKindCard);
  fixture.componentRef.setInput('characterId', 'c1');
  for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  return { fixture, el: fixture.nativeElement as HTMLElement, client };
}

const radio = (el: HTMLElement, value: string) =>
  el.querySelector(`input[type="radio"][value="${value}"]`) as HTMLInputElement;
const fieldset = (el: HTMLElement) => el.querySelector('fieldset') as HTMLFieldSetElement;
const text = (n: Element | null | undefined) => n?.textContent?.replace(/\s+/g, ' ').trim();

async function flush(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

function choose(el: HTMLElement, value: string): void {
  const r = radio(el, value);
  r.checked = true;
  r.dispatchEvent(new Event('change'));
}

function lastToast(): { type: string; message: string } | undefined {
  const t = TestBed.inject(ToastService).toasts().at(-1);
  return t && { type: t.type, message: t.message };
}

describe('CharacterKindCard (v4 b09ca46bb CharacterKindPicker)', () => {
  afterEach(() => vi.restoreAllMocks());

  it("carries v4's legend, hint, options and markup", () => {
    const { el } = render(async () => ({}));
    const card = el.querySelector('div.qt-card[data-testid="character-kind"]');
    expect(card).not.toBeNull();
    const legend = el.querySelector('fieldset > legend')!;
    expect(legend.className).toBe('qt-text-label flex items-center gap-3');
    expect(text(legend)).toBe('Kind of character');
    const hint = el.querySelector('fieldset > p')!;
    expect(hint.className).toBe('qt-text-small qt-text-secondary mt-0.5 mb-2');
    expect(text(hint)).toBe(
      'Whether this character is a player in the drama or a member of the staff. It decides how they are directed when the curtain is up.',
    );
    expect(fieldset(el).getAttribute('aria-describedby')).toBe(hint.id);
    expect(el.querySelector('fieldset > div')!.className).toBe('flex flex-col gap-1');
    const labels = Array.from(el.querySelectorAll('fieldset label'));
    expect(labels.map(text)).toEqual([
      'Roleplay character — a person in the story, with stakes of their own, who feels what happens and acts on it',
      'Assistant — a capable aide whose business is doing work for you: research, drafting, keeping the books',
    ]);
    expect(labels.map((l) => l.className)).toEqual([
      'flex items-center gap-2 text-sm cursor-pointer',
      'flex items-center gap-2 text-sm cursor-pointer',
    ]);
    expect(Array.from(labels[0].querySelectorAll('span span')).map((s) => s.className)).toEqual([
      'qt-text-xs qt-text-secondary',
    ]);
    expect(text(labels[0].querySelector('span span'))).toBe(
      '— a person in the story, with stakes of their own, who feels what happens and acts on it',
    );
    expect(radio(el, 'roleplay').classList.contains('qt-radio')).toBe(true);
    expect(radio(el, 'roleplay').name).toBe(radio(el, 'assistant').name);
    expect(radio(el, 'roleplay').name).not.toBe('');
  });

  it.each([[undefined], ['butler'], [null], ['Assistant']])(
    'an unrecognised kind %p shows Roleplay checked',
    (kind) => {
      const { el } = render(async () => ({}), { kind });
      expect(radio(el, 'roleplay').checked).toBe(true);
      expect(radio(el, 'assistant').checked).toBe(false);
    },
  );

  it('a stored assistant shows Assistant checked', () => {
    const { el } = render(async () => ({}), { kind: 'assistant' });
    expect(radio(el, 'assistant').checked).toBe(true);
  });

  it("choosing Assistant → characterUpdate {kind:'assistant'} and v4's toast", async () => {
    const dispatchData = vi.fn(async () => ({}));
    const { el } = render(dispatchData);
    choose(el, 'assistant');
    await flush();
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'characterUpdate',
      characterId: 'c1',
      character: { kind: 'assistant' },
    });
    expect(lastToast()).toEqual({
      type: 'success',
      message: 'Very good — this character joins the staff as an assistant',
    });
  });

  it("choosing Roleplay → {kind:'roleplay'} and the player toast", async () => {
    const dispatchData = vi.fn(async () => ({}));
    const { el } = render(dispatchData, { kind: 'assistant' });
    choose(el, 'roleplay');
    await flush();
    expect(dispatchData).toHaveBeenCalledWith({
      type: 'characterUpdate',
      characterId: 'c1',
      character: { kind: 'roleplay' },
    });
    expect(lastToast()).toEqual({
      type: 'success',
      message: 'Very good — this character takes the stage as a player in the drama',
    });
  });

  it('a refused save toasts the FIXED sentence, logs, and snaps the radio back', async () => {
    const err = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const dispatchData = vi.fn(async () => {
      throw new CoreDispatchError({ kind: 'validation', message: 'Validation error: kind' });
    });
    const { el, fixture } = render(dispatchData);
    choose(el, 'assistant');
    await flush();
    fixture.detectChanges();
    expect(lastToast()).toEqual({ type: 'error', message: 'Failed to update the kind of character' });
    expect(err).toHaveBeenCalledWith('Failed to save character kind', expect.objectContaining({}));
    expect(radio(el, 'roleplay').checked).toBe(true);
    expect(radio(el, 'assistant').checked).toBe(false);
  });

  it('a transport error toasts its own message', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { el } = render(async () => {
      throw new Error('offline');
    });
    choose(el, 'assistant');
    await flush();
    expect(lastToast()).toEqual({ type: 'error', message: 'offline' });
  });

  it('a refused save re-fetches the character detail', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const { el, client } = render(async () => {
      throw new CoreDispatchError({ kind: 'validation', message: 'no' });
    });
    const inv = vi.spyOn(client, 'invalidateQueries');
    choose(el, 'assistant');
    await flush();
    expect(inv).toHaveBeenCalledWith({ queryKey: ['characters', 'detail', 'c1'] });
  });

  it('is inert while saving: the spinner shows and the fieldset is disabled', async () => {
    let release!: () => void;
    const { el, fixture } = render(
      () =>
        new Promise((r) => {
          release = () => r({});
        }),
    );
    choose(el, 'assistant');
    fixture.detectChanges();
    expect(fieldset(el).disabled).toBe(true);
    expect(el.querySelector('legend span.qt-spinner')).not.toBeNull();
    expect(el.querySelector('fieldset label')!.className).toBe(
      'flex items-center gap-2 text-sm cursor-not-allowed qt-text-muted',
    );
    release();
    await flush();
    fixture.detectChanges();
    expect(fieldset(el).disabled).toBe(false);
    expect(el.querySelector('legend span.qt-spinner')).toBeNull();
  });

  it('is disabled with a null character id (loading) and when archived', () => {
    const a = render(async () => ({}), { characterId: null });
    expect(fieldset(a.el).disabled).toBe(true);
    TestBed.resetTestingModule();
    const b = render(async () => ({}), { disabled: true });
    expect(fieldset(b.el).disabled).toBe(true);
    expect(radio(b.el, 'roleplay').disabled).toBe(true);
  });

  it('two instances get DIFFERENT radio-group names; choosing in one leaves the other alone', () => {
    @Component({
      imports: [CharacterKindCard],
      template: `
        <qt-character-kind-card id="one" [characterId]="'c1'" [kind]="kind()" />
        <qt-character-kind-card id="two" [characterId]="'c1'" [kind]="kind()" />
      `,
    })
    class Host {
      readonly kind = signal<unknown>('roleplay');
    }
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: { dispatchData: vi.fn(() => new Promise(() => undefined)) } },
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const root = fixture.nativeElement as HTMLElement;
    const one = root.querySelector('#one') as HTMLElement;
    const two = root.querySelector('#two') as HTMLElement;
    expect(radio(one, 'roleplay').name).not.toBe(radio(two, 'roleplay').name);
    expect(one.querySelector('fieldset p')!.id).not.toBe(two.querySelector('fieldset p')!.id);
    choose(one, 'assistant');
    expect(radio(two, 'roleplay').checked).toBe(true);
  });
});
