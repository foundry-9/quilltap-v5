import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { AllLLMPauseModal, type AllLLMPauseParticipant } from './all-llm-pause-modal';

const CAST: AllLLMPauseParticipant[] = [
  { id: 'p1', characterName: 'Ariel', avatarUrl: null },
  { id: 'p2', characterName: 'Prospero', avatarUrl: null },
];

function render(
  over: { turnCount?: number; nextPauseAt?: number; participants?: AllLLMPauseParticipant[] } = {},
): ComponentFixture<AllLLMPauseModal> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [AllLLMPauseModal] });
  const fixture = TestBed.createComponent(AllLLMPauseModal);
  fixture.componentRef.setInput('turnCount', over.turnCount ?? 6);
  fixture.componentRef.setInput('nextPauseAt', over.nextPauseAt ?? 12);
  fixture.componentRef.setInput('participants', over.participants ?? CAST);
  fixture.detectChanges();
  return fixture;
}

function buttonByText(fixture: ComponentFixture<AllLLMPauseModal>, text: string): HTMLButtonElement {
  // The take-over buttons carry the avatar's initial before the label, so match
  // on inclusion rather than prefix.
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.replace(/\s+/g, ' ').trim().includes(text),
  )!;
}

describe('AllLLMPauseModal', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders v4’s copy with the turn count and next-pause turn', () => {
    const text = render().nativeElement.textContent.replace(/\s+/g, ' ');
    expect(text).toContain('running automatically for 6 turns');
    expect(text).toContain('Pause intervals: 3, 6, 12, 24, 48...');
    expect(text).toContain('The next pause will occur at turn 12.');
  });

  it('labels Continue with the turns until the next pause (nextPauseAt − turnCount)', () => {
    const fixture = render({ turnCount: 6, nextPauseAt: 12 });
    expect(buttonByText(fixture, 'Continue').textContent!.replace(/\s+/g, ' ').trim()).toBe(
      'Continue (6 more turns)',
    );
  });

  it('emits continueRun with the remaining turns, then close', () => {
    const fixture = render({ turnCount: 6, nextPauseAt: 12 });
    const events: string[] = [];
    let turns = -1;
    fixture.componentInstance.continueRun.subscribe((n) => {
      turns = n;
      events.push('continue');
    });
    fixture.componentInstance.close.subscribe(() => events.push('close'));
    buttonByText(fixture, 'Continue').click();
    expect(turns).toBe(6);
    expect(events).toEqual(['continue', 'close']);
  });

  it('emits stopRun then close on Stop', () => {
    const fixture = render();
    const events: string[] = [];
    fixture.componentInstance.stopRun.subscribe(() => events.push('stop'));
    fixture.componentInstance.close.subscribe(() => events.push('close'));
    buttonByText(fixture, 'Stop').click();
    expect(events).toEqual(['stop', 'close']);
  });

  it('renders a take-over button per participant and emits takeOver(id) then close', () => {
    const fixture = render();
    const events: string[] = [];
    let takenOver = '';
    fixture.componentInstance.takeOver.subscribe((id) => {
      takenOver = id;
      events.push('takeOver');
    });
    fixture.componentInstance.close.subscribe(() => events.push('close'));

    const playAs = buttonByText(fixture, 'Play as Prospero');
    expect(playAs).toBeTruthy();
    playAs.click();
    expect(takenOver).toBe('p2');
    expect(events).toEqual(['takeOver', 'close']);
  });

  it('omits the take-over section when there are no participants', () => {
    const text = render({ participants: [] }).nativeElement.textContent;
    expect(text).not.toContain('Or take control of a character');
  });
});
