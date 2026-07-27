import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { RngDropdown, type RngPendingResult } from './rng-dropdown';

/** v4's preview body (`actions/rng.ts:91-107`) for a 1d20 that came up 17. */
const D20_PREVIEW = {
  success: true,
  preview: true,
  result: {
    type: 20,
    rollCount: 1,
    results: [17],
    sum: 17,
    formattedText: '🎲 Rolled 1d20: **17**',
    summary: 'd20: 17',
    requestPrompt: 'Roll a d20',
    arguments: { type: 20, rolls: 1 },
  },
};

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

function stub(body: Record<string, unknown> | Error = D20_PREVIEW): Stub {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    if (body instanceof Error) throw body;
    return body;
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

function mount(s: Stub, disabled = false): ComponentFixture<RngDropdown> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [RngDropdown],
    providers: [{ provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(RngDropdown);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('disabled', disabled);
  fixture.detectChanges();
  return fixture;
}

function open(fixture: ComponentFixture<RngDropdown>): void {
  (
    fixture.nativeElement.querySelector(
      '[aria-label="Random number generator"]',
    ) as HTMLButtonElement
  ).click();
  fixture.detectChanges();
}

function menuItem(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  return [...fixture.nativeElement.querySelectorAll('[role="menuitem"]')].find((b) =>
    (b.textContent ?? '').trim().startsWith(label),
  ) as HTMLButtonElement;
}

function byLabel(fixture: ComponentFixture<unknown>, label: string): HTMLElement {
  return fixture.nativeElement.querySelector(`[aria-label="${label}"]`) as HTMLElement;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
}

describe('RngDropdown (v4 components/chat/RngDropdown.tsx, gutter variant)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('opens onto v4’s menu: d6, d20, both named rolls, and Custom Roll', () => {
    const fixture = mount(stub());
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeNull();
    open(fixture);
    const body = text(fixture);
    expect(body).toContain('Roll 1d6');
    expect(body).toContain('Roll 1d20');
    expect(body).toContain('Flip Coin');
    expect(body).toContain('Spin the Bottle');
    expect(body).toContain('Custom Roll');
  });

  it('rolls in PREVIEW mode and sends `kind`, never `type` (E3A §1)', async () => {
    const s = stub();
    const fixture = mount(s);
    open(fixture);
    menuItem(fixture, 'Roll 1d20').click();
    await settle(fixture);
    expect(s.calls[0]).toEqual({
      type: 'chatRng',
      chatId: 'chat-1',
      kind: 20,
      rolls: 1,
      preview: true,
    });
  });

  it('emits v4’s pending-chip shape, arguments passed through untouched', async () => {
    const s = stub();
    const fixture = mount(s);
    const seen: RngPendingResult[] = [];
    fixture.componentInstance.pendingResult.subscribe((r) => seen.push(r));
    open(fixture);
    menuItem(fixture, 'Roll 1d20').click();
    await settle(fixture);
    expect(seen).toEqual([
      {
        tool: 'rng',
        displayName: 'Random Number Generator',
        icon: '🎲',
        summary: 'd20: 17',
        formattedResult: '🎲 Rolled 1d20: **17**',
        requestPrompt: 'Roll a d20',
        // v4's own {type, rolls} — the persisted TOOL row's arguments.
        arguments: { type: 20, rolls: 1 },
        success: true,
      },
    ]);
  });

  it('closes the menu after a successful roll', async () => {
    const fixture = mount(stub());
    open(fixture);
    menuItem(fixture, 'Roll 1d20').click();
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeNull();
  });

  it('the ± spinners move the die count, clamped at 1 and 100', () => {
    const fixture = mount(stub());
    open(fixture);
    const up = byLabel(fixture, 'Increase d20 count') as HTMLButtonElement;
    const down = byLabel(fixture, 'Decrease d20 count') as HTMLButtonElement;
    // At 1 the decrement is disabled (v4 :225).
    expect(down.disabled).toBe(true);
    up.click();
    up.click();
    fixture.detectChanges();
    expect(text(fixture)).toContain('Roll 3d20');
    for (let i = 0; i < 200; i++) {
      (byLabel(fixture, 'Increase d20 count') as HTMLButtonElement).click();
    }
    fixture.detectChanges();
    expect(text(fixture)).toContain('Roll 100d20');
    expect((byLabel(fixture, 'Increase d20 count') as HTMLButtonElement).disabled).toBe(true);
  });

  it('rolls the adjusted count, and the d6 and d20 counts are independent', async () => {
    const s = stub();
    const fixture = mount(s);
    open(fixture);
    (byLabel(fixture, 'Increase d6 count') as HTMLButtonElement).click();
    fixture.detectChanges();
    menuItem(fixture, 'Roll 2d6').click();
    await settle(fixture);
    expect(s.calls[0]).toMatchObject({ kind: 6, rolls: 2 });

    open(fixture);
    expect(text(fixture)).toContain('Roll 1d20');
  });

  it('sends the two named kinds with one roll each', async () => {
    const s = stub();
    const fixture = mount(s);
    open(fixture);
    menuItem(fixture, 'Flip Coin').click();
    await settle(fixture);
    expect(s.calls[0]).toMatchObject({ kind: 'flip_coin', rolls: 1 });

    open(fixture);
    menuItem(fixture, 'Spin the Bottle').click();
    await settle(fixture);
    expect(s.calls[1]).toMatchObject({ kind: 'spin_the_bottle', rolls: 1 });
  });

  it('refuses an out-of-range custom roll BEFORE any request (v4 :143-156)', async () => {
    const s = stub();
    const fixture = mount(s);
    open(fixture);
    menuItem(fixture, 'Custom Roll').click();
    fixture.detectChanges();

    const sides = byLabel(fixture, 'Number of sides') as HTMLInputElement;
    sides.value = '1';
    sides.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    (menuItem(fixture, 'Custom Roll')!.parentElement!.querySelector(
      '.qt-button-primary',
    ) as HTMLButtonElement).click();
    await settle(fixture);
    expect(text(fixture)).toContain('Sides must be between 2 and 1000');
    expect(s.calls).toHaveLength(0);

    sides.value = '100';
    sides.dispatchEvent(new Event('input'));
    const rolls = byLabel(fixture, 'Number of rolls') as HTMLInputElement;
    rolls.value = '101';
    rolls.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    (menuItem(fixture, 'Custom Roll')!.parentElement!.querySelector(
      '.qt-button-primary',
    ) as HTMLButtonElement).click();
    await settle(fixture);
    expect(text(fixture)).toContain('Rolls must be between 1 and 100');
    expect(s.calls).toHaveLength(0);
  });

  it('sends a valid custom roll', async () => {
    const s = stub();
    const fixture = mount(s);
    open(fixture);
    menuItem(fixture, 'Custom Roll').click();
    fixture.detectChanges();
    const sides = byLabel(fixture, 'Number of sides') as HTMLInputElement;
    sides.value = '1000';
    sides.dispatchEvent(new Event('input'));
    const rolls = byLabel(fixture, 'Number of rolls') as HTMLInputElement;
    rolls.value = '3';
    rolls.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    (menuItem(fixture, 'Custom Roll')!.parentElement!.querySelector(
      '.qt-button-primary',
    ) as HTMLButtonElement).click();
    await settle(fixture);
    expect(s.calls[0]).toMatchObject({ kind: 1000, rolls: 3 });
  });

  it('keeps the menu open and shows the server’s message on failure', async () => {
    const s = stub(new Error('unknown variant `chatRng`'));
    const fixture = mount(s);
    const seen: RngPendingResult[] = [];
    fixture.componentInstance.pendingResult.subscribe((r) => seen.push(r));
    open(fixture);
    menuItem(fixture, 'Roll 1d20').click();
    await settle(fixture);
    expect(text(fixture)).toContain('unknown variant');
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeTruthy();
    expect(seen).toEqual([]);
  });

  it('dismisses on a press outside, closing the custom panel with it (v4 useClickOutside)', () => {
    const fixture = mount(stub());
    open(fixture);
    // Open the custom sub-panel too — v4 closes BOTH.
    (
      [...fixture.nativeElement.querySelectorAll('button')].find((b) =>
        (b.textContent ?? '').includes('Custom Roll'),
      ) as HTMLButtonElement
    ).click();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeTruthy();

    // A press INSIDE leaves it alone.
    (fixture.nativeElement.querySelector('[role="menu"]') as HTMLElement).dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true }),
    );
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeTruthy();

    // A press OUTSIDE closes it — v4 listens on mousedown, not click.
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeNull();

    // …and the custom panel did not survive to the next open.
    open(fixture);
    expect(text(fixture)).not.toContain('Sides');
  });

  it('swaps the die for a spinner while a roll is in flight (v4 :187-195)', async () => {
    let release: ((v: Record<string, unknown>) => void) | undefined;
    const client: Partial<CoreClient> = {
      dispatchData: (() =>
        new Promise((resolve) => {
          release = resolve;
        })) as unknown as CoreClient['dispatchData'],
    };
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [RngDropdown],
      providers: [{ provide: CoreClient, useValue: client }],
    });
    const fixture = TestBed.createComponent(RngDropdown);
    fixture.componentRef.setInput('chatId', 'chat-1');
    fixture.detectChanges();

    const trigger = () =>
      fixture.nativeElement.querySelector(
        '[aria-label="Random number generator"]',
      ) as HTMLButtonElement;
    expect(trigger().querySelector('qt-icon')).toBeTruthy();
    expect(trigger().querySelector('.qt-spinner-sm')).toBeNull();

    open(fixture);
    menuItem(fixture, 'Roll 1d20').click();
    fixture.detectChanges();

    expect(trigger().querySelector('.qt-spinner-sm')).toBeTruthy();
    expect(trigger().querySelector('qt-icon')).toBeNull();

    release!(D20_PREVIEW);
    await settle(fixture);
    expect(trigger().querySelector('.qt-spinner-sm')).toBeNull();
    expect(trigger().querySelector('qt-icon')).toBeTruthy();
  });

  it('does not open at all while the composer is disabled', () => {
    const fixture = mount(stub(), true);
    open(fixture);
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeNull();
  });
});
