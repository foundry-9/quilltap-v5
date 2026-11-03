/**
 * The name chips on a rendered message (P4.D359 item 10). The first three
 * cases TRANSCRIBE v4 `components/chat/__tests__/MentionChip.test.tsx:46-82`
 * at `236c04e35` over `<qt-message-content [mentions]>` — the real renderer,
 * the real `message-content.ts` hook (P4.D353's fence) and the chips'
 * directive. The rest pin v5's delegated behaviour (`MENTION_CARD_SHARED_POPOVER`).
 */
import { ChangeDetectionStrategy, Component, signal } from '@angular/core';
import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AbsentMention } from '../../core/core-contract';
import { FLOATING_LAYER_ATTR } from '../../ui/floating-layer';
import { MessageContent } from '../message-content';
import { MentionChipHost } from './mention-chip-host';

const abby: AbsentMention = {
  messageId: 'm1',
  characterId: 'abigail',
  text: 'Abby',
  card: {
    name: 'Abigail',
    aliases: ['Abby', 'True North'],
    pronouns: 'she/her/her',
    body: 'Abigail keeps the records.',
  },
};
const greg: AbsentMention = {
  messageId: 'm1',
  characterId: 'greg',
  text: 'Greg',
  card: { name: 'Gregory', aliases: [], pronouns: null, body: '' },
};

@Component({
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MessageContent, MentionChipHost],
  template: `<qt-message-content [content]="content()" [mentions]="mentions()" />`,
})
class Host {
  readonly content = signal('');
  readonly mentions = signal<readonly AbsentMention[] | null>(null);
}

let fixture: ComponentFixture<Host>;

async function render(
  content: string,
  mentions: readonly AbsentMention[] | null,
): Promise<HTMLElement> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [Host] });
  fixture = TestBed.createComponent(Host);
  fixture.componentInstance.content.set(content);
  fixture.componentInstance.mentions.set(mentions);
  document.body.appendChild(fixture.nativeElement);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

const card = () => document.querySelector('qt-mention-card [role="tooltip"]') as HTMLElement | null;
const chip = (el: HTMLElement, name = 'Abigail') =>
  el.querySelector(`[aria-label="${name}, not in this conversation"]`) as HTMLElement;

afterEach(() => {
  vi.useRealTimers();
  fixture?.destroy();
  (fixture?.nativeElement as HTMLElement | undefined)?.remove();
  document.querySelectorAll('qt-mention-card').forEach((c) => c.remove());
});

describe('MentionChip in MessageContent (transcribed — v4 MentionChip.test.tsx)', () => {
  it('chips the first mention inside the dialogue styling, and only once', async () => {
    const el = await render('"When Abby is not scolding Vergil." Abby grins.', [abby]);
    const chips = el.querySelectorAll('.qt-mention-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0].textContent).toBe('Abby');
    // The quote is still one styled dialogue run, with the chip inside it.
    const dialogue = chips[0].closest('.qt-chat-dialogue');
    expect(dialogue?.textContent).toBe('"When Abby is not scolding Vergil."');
    expect(el.textContent).not.toMatch(/[-]/);
  });

  it('shows the card on focus', async () => {
    const el = await render('Abby came by.', [abby]);
    chip(el).dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    const tooltip = card()!;
    expect(tooltip.textContent).toContain('Abigail');
    expect(tooltip.textContent).toContain('Abby, True North');
    expect(tooltip.textContent).toContain('she/her/her');
    expect(tooltip.textContent).toContain('Abigail keeps the records.');
  });

  it('renders plainly without mentions', async () => {
    const el = await render('Abby came by.', null);
    expect(el.querySelector('.qt-mention-chip')).toBeNull();
  });
});

describe('the chips’ delegated behaviour (v5)', () => {
  it('the card carries v4’s bytes and the tooltip role', async () => {
    const el = await render('Abby came by.', [abby]);
    chip(el).dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    const c = card()!;
    expect(c.getAttribute('role')).toBe('tooltip');
    expect(c.querySelector('.qt-tooltip-title')!.textContent).toBe('Abigail');
    expect([...c.querySelectorAll('.qt-tooltip-section-label')].map((l) => l.textContent)).toEqual([
      'Also known as',
      'Pronouns',
    ]);
    expect(c.querySelector('.qt-tooltip-hint')!.textContent).toBe(
      'Not in this conversation · click to pin',
    );
  });

  it('hover opens the card after 250 ms; leaving closes it after the grace', async () => {
    const el = await render('Abby came by.', [abby]);
    vi.useFakeTimers();
    chip(el).dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
    vi.advanceTimersByTime(249);
    expect(card()).toBeNull();
    vi.advanceTimersByTime(1);
    expect(card()).not.toBeNull();
    chip(el).dispatchEvent(
      new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body }),
    );
    vi.advanceTimersByTime(120);
    expect(card()).toBeNull();
  });

  it('a click pins (data-pinned); a click outside unpins; a click in the floating layer does not', async () => {
    const el = await render('Abby came by.', [abby]);
    chip(el).click();
    expect(card()!.getAttribute('data-pinned')).toBe('true');
    const layer = document.createElement('div');
    layer.setAttribute(FLOATING_LAYER_ATTR, '');
    document.body.appendChild(layer);
    layer.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    expect(card()).not.toBeNull();
    layer.remove();
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    expect(card()).toBeNull();
  });

  it('Escape unpins; a second click on the chip unpins too', async () => {
    const el = await render('Abby came by.', [abby]);
    chip(el).click();
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(card()).toBeNull();
    chip(el).click();
    expect(card()).not.toBeNull();
    chip(el).click();
    expect(card()).toBeNull();
  });

  it('Enter and Space on the focused chip pin it (WCAG R8)', async () => {
    const el = await render('Abby came by.', [abby]);
    const enter = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    chip(el).dispatchEvent(enter);
    expect(enter.defaultPrevented).toBe(true);
    expect(card()!.getAttribute('data-pinned')).toBe('true');
    chip(el).dispatchEvent(
      new KeyboardEvent('keydown', { key: ' ', bubbles: true, cancelable: true }),
    );
    expect(card()).toBeNull();
  });

  it('two chips share ONE card, re-aimed', async () => {
    const el = await render('Abby met Greg.', [abby, greg]);
    expect(el.querySelectorAll('.qt-mention-chip')).toHaveLength(2);
    chip(el).dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    expect(card()!.querySelector('.qt-tooltip-title')!.textContent).toBe('Abigail');
    chip(el, 'Gregory').dispatchEvent(new FocusEvent('focusin', { bubbles: true }));
    expect(document.querySelectorAll('qt-mention-card')).toHaveLength(1);
    expect(card()!.querySelector('.qt-tooltip-title')!.textContent).toBe('Gregory');
  });

  it('a re-render keeps the chip (the hook re-marks after every render)', async () => {
    const el = await render('Abby came by.', [abby]);
    fixture.componentInstance.content.set('Abby came by again.');
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(el.querySelectorAll('.qt-mention-chip')).toHaveLength(1);
    expect(el.textContent).toContain('again');
  });

  it('mentions arriving after the render chip the existing text', async () => {
    const el = await render('Abby came by.', null);
    fixture.componentInstance.mentions.set([abby]);
    fixture.detectChanges();
    await fixture.whenStable();
    expect(el.querySelectorAll('.qt-mention-chip')).toHaveLength(1);
  });
});
