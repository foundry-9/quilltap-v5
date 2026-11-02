import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { createInitialTurnState } from '../turn-order';
import { ParticipantsSection, type TurnPosition } from './participants-section';

/**
 * The participants section's turn-mode control (P4.D344) — v5-authored from
 * v4 `components/chat/ChatSidebar.tsx` at the pin `236c04e35` (`TurnModeControl`
 * `:770-808`, the invite checkbox `:860-873`; v4 ships no test for either),
 * every label, title and class byte for byte.
 */

interface Mounted {
  fixture: ComponentFixture<ParticipantsSection>;
  el: HTMLElement;
  positions: TurnPosition[];
  toggles: number;
}

function mount(isPaused: boolean, turnMode: 'rotation' | 'storyteller', invite = false): Mounted {
  TestBed.configureTestingModule({ imports: [ParticipantsSection] });
  const fixture = TestBed.createComponent(ParticipantsSection);
  fixture.componentRef.setInput('sortedParticipants', []);
  fixture.componentRef.setInput('turnOrder', []);
  fixture.componentRef.setInput('turnState', createInitialTurnState());
  fixture.componentRef.setInput('isPaused', isPaused);
  fixture.componentRef.setInput('turnMode', turnMode);
  fixture.componentRef.setInput('storytellerInviteFilter', invite);
  const m: Mounted = { fixture, el: fixture.nativeElement as HTMLElement, positions: [], toggles: 0 };
  fixture.componentInstance.setTurnPosition.subscribe((p) => m.positions.push(p));
  fixture.componentInstance.toggleStorytellerInviteFilter.subscribe(() => m.toggles++);
  fixture.detectChanges();
  return m;
}

function radios(el: HTMLElement): HTMLButtonElement[] {
  return Array.from(el.querySelectorAll('[role="radiogroup"] [role="radio"]'));
}

describe('ParticipantsSection — the Turn mode control (v4 ChatSidebar.tsx @ 236c04e35)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('is a labelled radiogroup of three buttons with v4\'s labels and titles', () => {
    const { el } = mount(false, 'rotation');
    const group = el.querySelector('[role="radiogroup"]');
    expect(group?.getAttribute('aria-label')).toBe('Turn mode');
    expect(group?.getAttribute('class')).toBe('qt-storyteller-mode mt-3');
    expect(radios(el).map((r) => [r.textContent?.trim(), r.title, r.type])).toEqual([
      ['Normal', 'The company takes turns on its own', 'button'],
      ['Paused', 'Nobody answers until you nudge them', 'button'],
      ['Storyteller', 'Each of your posts asks the company who wishes to answer; you choose', 'button'],
    ]);
  });

  it('the current position for each (isPaused, turnMode) pair, and its class', () => {
    const cases: Array<[boolean, 'rotation' | 'storyteller', string[], string[]]> = [
      [false, 'rotation', ['true', 'false', 'false'], ['qt-storyteller-mode-option qt-storyteller-mode-option-active', 'qt-storyteller-mode-option', 'qt-storyteller-mode-option']],
      [false, 'storyteller', ['false', 'false', 'true'], ['qt-storyteller-mode-option', 'qt-storyteller-mode-option', 'qt-storyteller-mode-option qt-storyteller-mode-option-active']],
      [true, 'rotation', ['false', 'true', 'false'], ['qt-storyteller-mode-option', 'qt-storyteller-mode-option qt-storyteller-mode-option-paused', 'qt-storyteller-mode-option']],
      [true, 'storyteller', ['false', 'true', 'false'], ['qt-storyteller-mode-option', 'qt-storyteller-mode-option qt-storyteller-mode-option-paused', 'qt-storyteller-mode-option']],
    ];
    for (const [paused, mode, checked, classes] of cases) {
      const { el } = mount(paused, mode);
      expect(radios(el).map((r) => r.getAttribute('aria-checked'))).toEqual(checked);
      expect(radios(el).map((r) => r.getAttribute('class'))).toEqual(classes);
      TestBed.resetTestingModule();
    }
  });

  it('clicking another position emits it; clicking the active one emits nothing', () => {
    const m = mount(false, 'rotation');
    radios(m.el)[0].click();
    expect(m.positions).toEqual([]);
    radios(m.el)[1].click();
    radios(m.el)[2].click();
    expect(m.positions).toEqual(['paused', 'storyteller']);
  });

  it('the expanded Pause button is gone', () => {
    const { el } = mount(true, 'rotation');
    expect(el.querySelector('.qt-chat-pause-button')).toBeNull();
    expect(el.textContent).not.toContain('Resume');
  });

  it('the invite checkbox renders only in storyteller mode, reflects the input and emits on change', () => {
    expect(mount(false, 'rotation').el.textContent).not.toContain('Invite by talkativeness');
    TestBed.resetTestingModule();

    const m = mount(false, 'storyteller', true);
    const label = m.el.querySelector('label');
    expect(label?.getAttribute('class')).toBe('flex items-center gap-2 cursor-pointer mt-2');
    expect(label?.title).toBe(
      'Each round, a quiet character may be left out — the likelier the less talkative. Kept-quiet seats can still be asked from the board.',
    );
    expect(label?.querySelector('span.qt-label')?.textContent).toBe('Invite by talkativeness');
    const box = label?.querySelector('input[type="checkbox"].qt-checkbox') as HTMLInputElement;
    expect(box.checked).toBe(true);
    box.dispatchEvent(new Event('change'));
    expect(m.toggles).toBe(1);

    m.fixture.componentRef.setInput('storytellerInviteFilter', false);
    m.fixture.detectChanges();
    expect(box.checked).toBe(false);
  });

  it('a paused storyteller chat keeps the checkbox (pause and turn mode are independent)', () => {
    const { el } = mount(true, 'storyteller');
    expect(el.textContent).toContain('Invite by talkativeness');
  });
});
