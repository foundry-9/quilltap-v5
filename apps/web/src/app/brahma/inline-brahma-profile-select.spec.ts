// v5-authored from v4 `components/brahma-console/InlineBrahmaProfileSelect.tsx` +
// `hooks/useConnectionProfiles.ts` (`c539c82e0`, at the pin `624d0d25c`) — v4
// ships the select without a test. P4.D388 Tier 1 item 9, Tier 2 item 16.
import { Component, signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ConnectionProfileDto } from '../core/core-contract';
import {
  InlineBrahmaProfileSelect,
  inlineBrahmaOptionLabel,
  toInlineBrahmaProfileOption,
} from './inline-brahma-profile-select';

const PROFILES = [
  { id: 'p1', name: 'Aria GPT', provider: 'OPENAI', modelName: 'gpt-5', isDefault: true, allowToolUse: true },
  { id: 'p2', name: 'Bare', provider: 'OLLAMA', modelName: '', isDefault: false, allowToolUse: false },
  { id: 'p3', name: 'Legacy', provider: 'ANTHROPIC', modelName: 'claude' },
] as unknown as ConnectionProfileDto[];

@Component({
  imports: [InlineBrahmaProfileSelect],
  template: `
    <qt-inline-brahma-profile-select
      [inputId]="inputId()"
      [ariaLabelledBy]="labelledBy()"
      [value]="value()"
      inheritLabel="Use my default profile"
      [disabled]="disabled()"
      [selectClass]="cls()"
      (profileChange)="emitted.push($event)"
    />
  `,
})
class Host {
  readonly inputId = signal<string | undefined>('brahma-inline-profile');
  readonly labelledBy = signal<string | undefined>(undefined);
  readonly value = signal<string | null | undefined>(null);
  readonly disabled = signal(false);
  readonly cls = signal('qt-select w-full max-w-md');
  readonly emitted: (string | null)[] = [];
}

async function render(
  value: string | null | undefined,
  profiles: Promise<ConnectionProfileDto[]> | ConnectionProfileDto[] = PROFILES,
): Promise<{ fixture: ComponentFixture<Host>; select: () => HTMLSelectElement }> {
  const dispatchExpect = vi.fn(async () => ({
    type: 'connectionProfiles',
    data: { profiles: await profiles },
  }));
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: { dispatchExpect } },
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.componentInstance.value.set(value);
  fixture.detectChanges();
  for (let i = 0; i < 3; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  const select = () => (fixture.nativeElement as HTMLElement).querySelector('select') as HTMLSelectElement;
  return { fixture, select };
}

const texts = (s: HTMLSelectElement) => Array.from(s.options).map((o) => o.textContent?.trim());

function choose(s: HTMLSelectElement, v: string): void {
  s.value = v;
  s.dispatchEvent(new Event('change'));
}

describe('inlineBrahmaOptionLabel (v4 InlineBrahmaProfileSelect option text)', () => {
  it.each([
    [{ name: 'Aria GPT', provider: 'OPENAI', modelName: 'gpt-5', isDefault: true, allowToolUse: true }, 'Aria GPT (OPENAI • gpt-5) — your default'],
    [{ name: 'Bare', provider: 'OLLAMA', modelName: '', isDefault: false, allowToolUse: true }, 'Bare (OLLAMA)'],
    [{ name: 'Mute', provider: 'X', modelName: 'm', isDefault: false, allowToolUse: false }, 'Mute (X • m) — no tools'],
    [{ name: 'Both', provider: 'X', modelName: 'm', isDefault: true, allowToolUse: false }, 'Both (X • m) — your default — no tools'],
  ])('%o → %s', (p, text) => {
    expect(inlineBrahmaOptionLabel({ id: 'x', ...p })).toBe(text);
  });

  it("maps v4's useConnectionProfiles coercions (absent allowToolUse reads true)", () => {
    expect(toInlineBrahmaProfileOption({ id: 'p', name: null, provider: undefined } as unknown as ConnectionProfileDto)).toEqual({
      id: 'p',
      name: '',
      provider: '',
      modelName: '',
      isDefault: false,
      allowToolUse: true,
    });
    expect(toInlineBrahmaProfileOption({ id: 'p', isDefault: 'yes' } as unknown as ConnectionProfileDto).isDefault).toBe(false);
  });
});

describe('InlineBrahmaProfileSelect (v4 c539c82e0)', () => {
  it('lists the inherit option first, then every profile in order', async () => {
    const { select } = await render(null);
    expect(texts(select())).toEqual([
      'Use my default profile',
      'Aria GPT (OPENAI • gpt-5) — your default',
      'Bare (OLLAMA) — no tools',
      'Legacy (ANTHROPIC • claude)',
    ]);
    expect(select().value).toBe('');
    expect(select().id).toBe('brahma-inline-profile');
    expect(select().getAttribute('class')).toBe('qt-select w-full max-w-md');
  });

  it('shows the stored profile as chosen', async () => {
    const { select } = await render('p2');
    expect(select().value).toBe('p2');
  });

  it('a stored id naming no profile shows "(a profile that has since left the building)", chosen', async () => {
    const { select } = await render('gone');
    expect(texts(select())[1]).toBe('(a profile that has since left the building)');
    expect(select().options[1].value).toBe('gone');
    expect(select().value).toBe('gone');
  });

  it('shows no "left the building" option for inherit or a known id', async () => {
    for (const v of [null, undefined, 'p1']) {
      TestBed.resetTestingModule();
      const { select } = await render(v);
      expect(texts(select())).not.toContain('(a profile that has since left the building)');
    }
  });

  it('is disabled while the profiles load, with no "left the building" option yet', async () => {
    let release!: (p: ConnectionProfileDto[]) => void;
    const pending = new Promise<ConnectionProfileDto[]>((r) => (release = r));
    const { fixture, select } = await render('gone', pending);
    expect(select().disabled).toBe(true);
    expect(texts(select())).toEqual(['Use my default profile']);
    release(PROFILES);
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(select().disabled).toBe(false);
  });

  it('is disabled when the caller says so', async () => {
    const { fixture, select } = await render(null);
    fixture.componentInstance.disabled.set(true);
    fixture.detectChanges();
    expect(select().disabled).toBe(true);
  });

  it('emits the id, and null for the inherit choice', async () => {
    const { fixture, select } = await render('p1');
    choose(select(), 'p2');
    choose(select(), '');
    expect(fixture.componentInstance.emitted).toEqual(['p2', null]);
  });

  it('is CONTROLLED: the DOM snaps back to the stored value until the input changes', async () => {
    const { fixture, select } = await render('p1');
    choose(select(), 'p2');
    fixture.detectChanges();
    expect(select().value).toBe('p1');
    fixture.componentInstance.value.set('p2');
    fixture.detectChanges();
    expect(select().value).toBe('p2');
  });

  it('carries aria-labelledby and a class override', async () => {
    const { fixture, select } = await render(null);
    fixture.componentInstance.inputId.set(undefined);
    fixture.componentInstance.labelledBy.set('qt-pmb-1-f3');
    fixture.componentInstance.cls.set('qt-input w-full max-w-xs');
    fixture.detectChanges();
    expect(select().hasAttribute('id')).toBe(false);
    expect(select().getAttribute('aria-labelledby')).toBe('qt-pmb-1-f3');
    expect(select().getAttribute('class')).toBe('qt-input w-full max-w-xs');
  });
});
