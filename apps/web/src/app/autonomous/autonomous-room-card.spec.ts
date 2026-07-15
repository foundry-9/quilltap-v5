import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { AutonomousRoomCard } from './autonomous-room-card';
import { INITIAL_AUTONOMOUS_STATE, type NewChatAutonomousState } from './autonomous.logic';

function render(scheduleCron: string): ComponentFixture<AutonomousRoomCard> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [AutonomousRoomCard] });
  const fixture = TestBed.createComponent(AutonomousRoomCard);
  const value: NewChatAutonomousState = { ...INITIAL_AUTONOMOUS_STATE, scheduleCron };
  fixture.componentRef.setInput('value', value);
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<AutonomousRoomCard>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

/**
 * The cron next-run preview (v4 `AutonomousRoomCard.tsx` L77-107). The three arms
 * carry v4's exact strings; the preview itself is proven in
 * `autonomous.logic.spec.ts`.
 */
describe('AutonomousRoomCard cron preview', () => {
  it('previews nothing while the cron field is blank', () => {
    const fixture = render('');
    expect(text(fixture)).not.toContain('Next run:');
    expect(text(fixture)).not.toContain('Invalid cron:');
    expect(text(fixture)).not.toContain('Parses, but never fires from now.');
  });

  it('shows the next fire time for a valid cron', () => {
    const fixture = render('0 4 * * *');
    expect(text(fixture)).toContain('Next run:');
    expect(text(fixture)).not.toContain('Invalid cron:');
  });

  it("shows croner's error for an unparseable cron", () => {
    const fixture = render('nonsense');
    expect(text(fixture)).toContain('Invalid cron:');
    expect(text(fixture)).not.toContain('Next run:');
  });

  it('shows the never-fires arm for a cron that parses but never comes around', () => {
    const fixture = render('0 0 30 2 *');
    expect(text(fixture)).toContain('Parses, but never fires from now.');
    expect(text(fixture)).not.toContain('Next run:');
  });
});
