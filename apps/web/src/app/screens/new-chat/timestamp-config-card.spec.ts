import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import type { TimestampConfig } from '../../core/core-contract';
import { TimestampConfigCard } from './timestamp-config-card';

/**
 * The two fictional-clock strings v4 rewrote in `e3a9654f`. Both of the ones it
 * replaced promised the clock "advances with each message" — something it had
 * never done, since nothing wrote `fictionalBaseRealTime` and the clock reported
 * the same instant forever. Pinned here because they are the only user-visible
 * half of P4.d18 and nothing else asserts them (no e2e beat reads this card).
 */
function render(value: TimestampConfig | null): ComponentFixture<TimestampConfigCard> {
  // Reset first: a test that renders the card twice (off, then on) would
  // otherwise reconfigure an already-instantiated TestBed.
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [TimestampConfigCard] });
  const fixture = TestBed.createComponent(TimestampConfigCard);
  fixture.componentRef.setInput('value', value);
  fixture.detectChanges();
  return fixture;
}

/** Collapse the template's line wrapping so the assertion reads as one sentence. */
function text(fixture: ComponentFixture<TimestampConfigCard>): string {
  return (fixture.nativeElement as HTMLElement).textContent!.replace(/\s+/g, ' ');
}

describe('TimestampConfigCard fictional-clock copy (v4 e3a9654f)', () => {
  it('describes the fictional clock as keeping pace with the real one', () => {
    const rendered = text(render({ mode: 'EVERY_MESSAGE' } as TimestampConfig));

    expect(rendered).toContain(
      'Instead of real-time, start the clock at a fictional moment. It then keeps pace with the real one, hour for hour',
    );
    expect(rendered).not.toContain('advances with each message');
  });

  it('explains the base timestamp once a fictional clock is switched on', () => {
    const off = text(render({ mode: 'EVERY_MESSAGE', useFictionalTime: false } as TimestampConfig));
    expect(off).not.toContain('Fictional Base Timestamp');

    const on = text(
      render({
        mode: 'EVERY_MESSAGE',
        useFictionalTime: true,
        fictionalBaseTimestamp: '1550-07-25T10:15',
      } as TimestampConfig),
    );
    expect(on).toContain(
      'The hour at which the tale begins, read as a clock in the timezone set above. From there it advances one minute for every minute that passes in the waking world; the clock starts when the chat is created.',
    );
  });
});
